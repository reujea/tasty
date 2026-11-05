import { kitCss } from "../../lib/vendor-assets";

export const GET = () => new Response(kitCss(), { headers: { "Content-Type": "text/css" } });
