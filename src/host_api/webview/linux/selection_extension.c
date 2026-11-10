/* WebKitGTK web-process extension for the Linux edit bindings. Copy and Cut must never replace the
 * clipboard with an empty selection, and a bound edit key must act after the input typed before it.
 *
 * - Ordered path: the UI arms a bound key (nonce, key code, modifiers) and lets the key
 *   enter WebKit's input queue. A native capture keydown listener on every document matches the armed
 *   key in input order, cancels its default action and later propagation, and tells the view whether
 *   that document has a range selection. The UI then runs the editing command. The listener is native,
 *   so it works while page scripts are disabled and never runs page script.
 * - Claimed keys: every armed combination stays claimed until the UI unclaims it. A claimed key that
 *   matches no live arm is cancelled without a command. A bound Copy or Cut that finds no range
 *   selection does not act, so it is not cancelled and reaches the engine default and the page.
 * - Direct query: for commands the host starts without a key, the UI asks for the focused frame's
 *   selection.
 * Messages carry only nonces, the page id, key codes, modifier bits and one boolean, never content. Contract: docs/design/systems/webview.md, editing actions section. */
#include <webkit2/webkit-web-extension.h>

#define QUERY_NAME "tasty-selection-query"
#define REPLY_NAME "tasty-selection-reply"
#define ARM_NAME "tasty-edit-arm"
#define ARMED_NAME "tasty-edit-armed"
#define UNCLAIM_NAME "tasty-edit-unclaim"
#define KEY_NAME "tasty-edit-key"
#define READY_NAME "tasty-edit-ready"
#define STATE_KEY "tasty-edit-state"
#define INSTALLED_KEY "tasty-edit-installed"
/* At most this many arms wait. One more arm overflows the page: every waiting arm is dropped and no
 * arm is accepted again while this page lives (a document load does not reset it). Claims stay, so
 * every claimed key is still cancelled without a command and none can take a later arm. */
#define MAX_ARMS 64

/* Modifier bits shared with selection.rs, compared exactly. The UI adds GDK META_MASK to keys with
 * Super, Hyper or MOD4 before WebKit sees them, so DOM metaKey reports those modifiers. */
#define MOD_CTRL 1u
#define MOD_SHIFT 2u
#define MOD_ALT 4u
#define MOD_META 8u

/* A key WebKit gives no keyCode (0 for every Cyrillic letter, for example) is keyed by the non-ASCII
 * code point of its keyIdentifier "U+XXXX" with this bit set. selection.rs builds the same value from
 * the GDK keyval, so no key text crosses the message boundary. */
#define IDENTIFIER_KEY 0x800000u

typedef struct {
    guint64 nonce;
    guint key_code;
    guint mods;
    /* Copy and Cut act only on a range selection. Without one the key is not Tasty's: it goes on to
     * the engine default and the page handlers like any unbound key. */
    gboolean needs_range;
} Arm;

typedef struct {
    GQueue arms;
    /* (key_code << 8 | mods) of claimed combinations. */
    GHashTable *claims;
    gboolean overflowed;
} State;

static void
free_state(gpointer data)
{
    State *state = data;
    g_queue_clear_full(&state->arms, g_free);
    g_hash_table_destroy(state->claims);
    g_free(state);
}

static State *
state_of(WebKitWebPage *page)
{
    State *state = g_object_get_data(G_OBJECT(page), STATE_KEY);
    if (!state) {
        state = g_new0(State, 1);
        g_queue_init(&state->arms);
        state->claims = g_hash_table_new(g_direct_hash, g_direct_equal);
        g_object_set_data_full(G_OBJECT(page), STATE_KEY, state, free_state);
    }
    return state;
}

static gpointer
combo(guint key_code, guint mods)
{
    return GUINT_TO_POINTER((key_code << 8) | mods);
}

G_GNUC_BEGIN_IGNORE_DEPRECATIONS
/* WebKitDOM is deprecated since 2.40 but is the only API that needs neither page scripts nor the
 * window-object-cleared signal, which is not emitted while scripts are disabled. */
static gboolean
has_range(WebKitDOMDocument *doc)
{
    /* The focused frame decides: follow active frames down to the document that has focus. A frame
     * whose document is out of reach has no confirmed range. Frame nesting is finite. */
    for (;;) {
        WebKitDOMElement *active = webkit_dom_document_get_active_element(doc);
        if (active && WEBKIT_DOM_IS_HTML_IFRAME_ELEMENT(active))
            doc = webkit_dom_html_iframe_element_get_content_document(WEBKIT_DOM_HTML_IFRAME_ELEMENT(active));
        else if (active && WEBKIT_DOM_IS_HTML_FRAME_ELEMENT(active))
            doc = webkit_dom_html_frame_element_get_content_document(WEBKIT_DOM_HTML_FRAME_ELEMENT(active));
        else
            break;
        if (!doc)
            return FALSE;
    }
    WebKitDOMDOMWindow *window = webkit_dom_document_get_default_view(doc);
    if (!window)
        return FALSE;
    WebKitDOMDOMSelection *selection = webkit_dom_dom_window_get_selection(window);
    g_object_unref(window);
    if (!selection)
        return FALSE;
    /* "Range" also covers a selection inside a text field; isCollapsed is wrong there. */
    gchar *type = webkit_dom_dom_selection_get_selection_type(selection);
    gboolean range = g_strcmp0(type, "Range") == 0;
    g_free(type);
    g_object_unref(selection);
    return range;
}

/* Takes the oldest arm for this key. Arms are answered in input order: an arm the UI has since
 * invalidated still consumes its own queued key, and the UI drops that reply, so a newer arm is never
 * taken by an older key. Arms ahead of the match belong to keys a page stopped before this listener
 * and are dropped. */
static Arm *
take_arm(State *state, guint key_code, guint mods)
{
    for (GList *l = state->arms.head; l; l = l->next) {
        Arm *arm = l->data;
        if (arm->key_code == key_code && arm->mods == mods) {
            Arm *ahead;
            while ((ahead = g_queue_peek_head(&state->arms)) != arm)
                g_free(g_queue_pop_head(&state->arms));
            return g_queue_pop_head(&state->arms);
        }
    }
    return NULL;
}

static guint
identifier_key(WebKitDOMKeyboardEvent *key)
{
    gchar *id = webkit_dom_keyboard_event_get_key_identifier(key);
    guint result = 0;
    if (id && id[0] == 'U' && id[1] == '+' && g_ascii_isxdigit(id[2])) {
        gchar *end = NULL;
        guint64 cp = g_ascii_strtoull(id + 2, &end, 16);
        if (end && *end == '\0' && cp > 0x7F && cp <= 0x10FFFF)
            result = IDENTIFIER_KEY | (guint)cp;
    }
    g_free(id);
    return result;
}

static void
on_keydown(WebKitDOMEventTarget *target, WebKitDOMEvent *event, gpointer data)
{
    WebKitWebPage *page = data;
    State *state = state_of(page);
    if (!WEBKIT_DOM_IS_KEYBOARD_EVENT(event))
        return;
    WebKitDOMKeyboardEvent *key = WEBKIT_DOM_KEYBOARD_EVENT(event);
    guint mods = (webkit_dom_keyboard_event_get_ctrl_key(key) ? MOD_CTRL : 0)
        | (webkit_dom_keyboard_event_get_shift_key(key) ? MOD_SHIFT : 0)
        | (webkit_dom_keyboard_event_get_alt_key(key) ? MOD_ALT : 0)
        | (webkit_dom_keyboard_event_get_meta_key(key) ? MOD_META : 0);
    guint key_code = (guint)webkit_dom_ui_event_get_key_code(WEBKIT_DOM_UI_EVENT(event));
    if (key_code == 0)
        key_code = identifier_key(key);
    if (key_code == 0)
        return;
    if (!g_hash_table_contains(state->claims, combo(key_code, mods)))
        return;
    Arm *arm = take_arm(state, key_code, mods);
    gboolean range = arm && WEBKIT_DOM_IS_DOCUMENT(target) && has_range(WEBKIT_DOM_DOCUMENT(target));
    /* A claimed edit key that Tasty runs belongs to Tasty: the engine default does not run, and page
     * handlers after the document capture phase do not see it, so they cannot change the selection
     * before the command runs. A claimed key with no live arm is cancelled without a command. A Copy
     * or Cut with nothing selected does not act, so it goes on to the engine and the page. */
    if (!arm || range || !arm->needs_range) {
        webkit_dom_event_prevent_default(event);
        webkit_dom_event_stop_propagation(event);
    }
    if (!arm)
        return;
    GVariant *params = g_variant_new("(ttb)", arm->nonce, webkit_web_page_get_id(page), range);
    g_free(arm);
    webkit_web_page_send_message_to_view(page, webkit_user_message_new(KEY_NAME, params), NULL, NULL, NULL);
}

static void ensure(WebKitWebPage *page, WebKitDOMDocument *doc);

static void
install_frame(WebKitWebPage *page, WebKitDOMNode *node)
{
    WebKitDOMDocument *inner = NULL;
    if (WEBKIT_DOM_IS_HTML_IFRAME_ELEMENT(node))
        inner = webkit_dom_html_iframe_element_get_content_document(WEBKIT_DOM_HTML_IFRAME_ELEMENT(node));
    else if (WEBKIT_DOM_IS_HTML_FRAME_ELEMENT(node))
        inner = webkit_dom_html_frame_element_get_content_document(WEBKIT_DOM_HTML_FRAME_ELEMENT(node));
    if (inner)
        ensure(page, inner);
}

/* A frame's keys dispatch on its own document, so each frame document gets the listener when the frame
 * fires load (a navigation inside the frame loads a new document again). */
static void
on_frame_load(G_GNUC_UNUSED WebKitDOMEventTarget *target, WebKitDOMEvent *event, gpointer data)
{
    WebKitDOMEventTarget *frame = webkit_dom_event_get_target(event);
    if (frame && WEBKIT_DOM_IS_NODE(frame))
        install_frame(data, WEBKIT_DOM_NODE(frame));
    g_clear_object(&frame);
}

static void
install_existing_frames(WebKitWebPage *page, WebKitDOMDocument *doc, const char *tag)
{
    WebKitDOMHTMLCollection *frames = webkit_dom_document_get_elements_by_tag_name_as_html_collection(doc, tag);
    if (!frames)
        return;
    gulong count = webkit_dom_html_collection_get_length(frames);
    for (gulong i = 0; i < count; i++) {
        WebKitDOMNode *node = webkit_dom_html_collection_item(frames, i);
        if (node)
            install_frame(page, node);
    }
    g_object_unref(frames);
}

/* Adds the listeners to one document once. */
static void
install(WebKitWebPage *page, WebKitDOMDocument *doc)
{
    if (g_object_get_data(G_OBJECT(doc), INSTALLED_KEY))
        return;
    g_object_set_data(G_OBJECT(doc), INSTALLED_KEY, GINT_TO_POINTER(1));
    WebKitDOMEventTarget *target = WEBKIT_DOM_EVENT_TARGET(doc);
    /* A WebKitDOM keydown listener on the window never fires (measured), so it sits on the document
     * in the capture phase. */
    webkit_dom_event_target_add_event_listener_with_closure(
        target, "keydown", g_cclosure_new(G_CALLBACK(on_keydown), page, NULL), TRUE);
    /* load does not bubble; the capture phase on the document sees a child frame's load. */
    webkit_dom_event_target_add_event_listener_with_closure(
        target, "load", g_cclosure_new(G_CALLBACK(on_frame_load), page, NULL), TRUE);
}

/* Installs on this document and every frame document under it that exists now, whether or not that
 * frame has fired load yet. The walk follows the finite frame tree. */
static void
ensure(WebKitWebPage *page, WebKitDOMDocument *doc)
{
    install(page, doc);
    install_existing_frames(page, doc, "iframe");
    install_existing_frames(page, doc, "frame");
}

static void
on_document_loaded(WebKitWebPage *page, G_GNUC_UNUSED gpointer data)
{
    WebKitDOMDocument *doc = webkit_web_page_get_dom_document(page);
    if (!doc)
        return;
    ensure(page, doc);
    /* Arms stay: a key armed before this document may still be queued and must consume its own arm
     * (the UI drops that reply). Claims stay too: they follow the bindings. */
    GVariant *params = g_variant_new("(t)", webkit_web_page_get_id(page));
    webkit_web_page_send_message_to_view(page, webkit_user_message_new(READY_NAME, params), NULL, NULL, NULL);
}

static gboolean
on_message(WebKitWebPage *page, WebKitUserMessage *message, G_GNUC_UNUSED gpointer data)
{
    const char *name = webkit_user_message_get_name(message);
    GVariant *params = webkit_user_message_get_parameters(message);
    State *state = state_of(page);
    if (g_strcmp0(name, ARM_NAME) == 0) {
        Arm arm = {0};
        if (!params || !g_variant_is_of_type(params, G_VARIANT_TYPE("(tuub)")))
            return FALSE;
        g_variant_get(params, "(tuub)", &arm.nonce, &arm.key_code, &arm.mods, &arm.needs_range);
        /* The arm arrives before its key, so every frame document that exists now, including one whose
         * frame is still loading, has the listener before the key is dispatched. */
        WebKitDOMDocument *doc = webkit_web_page_get_dom_document(page);
        if (doc)
            ensure(page, doc);
        /* Claim first, so a refused key is cancelled rather than reaching the engine's empty copy. */
        g_hash_table_add(state->claims, combo(arm.key_code, arm.mods));
        if (!state->overflowed && g_queue_get_length(&state->arms) >= MAX_ARMS) {
            state->overflowed = TRUE;
            g_queue_clear_full(&state->arms, g_free);
        }
        gboolean accepted = !state->overflowed;
        if (accepted)
            g_queue_push_tail(&state->arms, g_memdup2(&arm, sizeof arm));
        GVariant *reply = g_variant_new("(ttb)", arm.nonce, webkit_web_page_get_id(page), accepted);
        webkit_user_message_send_reply(message, webkit_user_message_new(ARMED_NAME, reply));
        return TRUE;
    }
    if (g_strcmp0(name, UNCLAIM_NAME) == 0) {
        guint key_code = 0, mods = 0;
        if (params && g_variant_is_of_type(params, G_VARIANT_TYPE("(uu)"))) {
            g_variant_get(params, "(uu)", &key_code, &mods);
            g_hash_table_remove(state->claims, combo(key_code, mods));
        }
        webkit_user_message_send_reply(message, webkit_user_message_new(UNCLAIM_NAME, NULL));
        return TRUE;
    }
    if (g_strcmp0(name, QUERY_NAME) != 0)
        return FALSE;
    guint64 nonce = 0;
    if (params && g_variant_is_of_type(params, G_VARIANT_TYPE("(t)")))
        g_variant_get(params, "(t)", &nonce);
    WebKitDOMDocument *doc = webkit_web_page_get_dom_document(page);
    gboolean range = doc && has_range(doc);
    GVariant *reply = g_variant_new("(ttb)", nonce, webkit_web_page_get_id(page), range);
    webkit_user_message_send_reply(message, webkit_user_message_new(REPLY_NAME, reply));
    return TRUE;
}
G_GNUC_END_IGNORE_DEPRECATIONS

static void
on_page_created(G_GNUC_UNUSED WebKitWebExtension *extension, WebKitWebPage *page, G_GNUC_UNUSED gpointer data)
{
    g_signal_connect(page, "document-loaded", G_CALLBACK(on_document_loaded), NULL);
    g_signal_connect(page, "user-message-received", G_CALLBACK(on_message), NULL);
}

G_MODULE_EXPORT void
webkit_web_extension_initialize(WebKitWebExtension *extension)
{
    g_signal_connect(extension, "page-created", G_CALLBACK(on_page_created), NULL);
}
