import base from "./playwright.config.mjs";
export default { ...base, projects: [{ name: "wk", testDir: "specs", use: { browserName: "webkit" } }], use: { ...base.use, browserName: "webkit" } };
