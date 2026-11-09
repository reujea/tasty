"""Build the prompt of one call from the stored templates. A hash pins each result."""

from common import FORMAT_NAMES, MISSING_MARK, REFERENCE_TIME, format_time, rng, sha256_bytes

# Only dot lines do not say what an edge means. Other formats name parents or children.
EDGE_NOTES = {"dot": "\n" + 'An edge "a" -> "b" means a is a parent of b.'}


def question_order(call_id, questions):
    """Return the question ids in this call's order. Retries reuse the same order."""
    ids = [q["id"] for q in questions]
    rng("order", call_id).shuffle(ids)
    return ids


def build_user_prompt(template, fmt, rendered, questions, minutes, order):
    by_id = {q["id"]: q for q in questions}
    lines = "\n".join(f"{qid}: {by_id[qid]['text']}" for qid in order)
    values = {
        "{format_name}": FORMAT_NAMES[fmt],
        "{missing_note}": f"A missing value is written as {MISSING_MARK[fmt]}.",
        "{edge_note}": EDGE_NOTES.get(fmt, ""),
        "{T}": format_time(REFERENCE_TIME),
        "{rendered}": rendered,
        "{N}": str(minutes),
        "{questions}": lines,
    }
    text = template
    for key, value in values.items():
        if text.count(key) != 1:
            raise ValueError(f"template placeholder count: {key}")
        text = text.replace(key, value)
    return text


def joined_prompt(system, user):
    """Return the single text Codex reads. Claude gets the two parts apart."""
    return f"{system}\n\n{user}"


def prompt_hash(system, user):
    return sha256_bytes(joined_prompt(system, user).encode())
