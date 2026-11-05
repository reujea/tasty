import { tokensCss } from "../../lib/vendor-assets";

export const GET = () => new Response(tokensCss(), { headers: { "Content-Type": "text/css" } });
