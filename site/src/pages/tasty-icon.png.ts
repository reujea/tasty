import { readFileSync } from "node:fs";
import { resolve } from "node:path";

/** The app icon lives in the repository's assets, outside the Astro project. */
export const GET = () =>
  new Response(readFileSync(resolve(process.cwd(), "../assets/icons/icon_256.png")), {
    headers: { "Content-Type": "image/png" },
  });
