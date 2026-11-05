import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/** The logo lives in the repository's assets, outside the Astro project. */
export const GET = () =>
  new Response(readFileSync(resolve(process.cwd(), "../assets/icons/tasty-melon.svg")), {
    headers: { "Content-Type": "image/svg+xml" },
  });
