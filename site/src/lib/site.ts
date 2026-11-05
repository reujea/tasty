export const REPO = "https://github.com/zilhak/tasty";

/** Join a site path to the configured deployment base. */
export const url = (path: string) =>
  `${import.meta.env.BASE_URL.replace(/\/$/, "")}/${path.replace(/^\//, "")}`;
