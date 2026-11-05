import type { APIRoute } from "astro";
import { cards } from "../../../lib/vendor-assets";

export const getStaticPaths = () => Object.keys(cards()).map((name) => ({ params: { name } }));

export const GET: APIRoute = ({ params }) =>
  new Response(cards()[params.name as string], { headers: { "Content-Type": "text/html; charset=utf-8" } });
