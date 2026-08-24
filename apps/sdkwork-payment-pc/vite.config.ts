import { resolveBrowserDistOutDir } from '../../../sdkwork-specs/tools/browser-dist-layout.mjs';
function resolveViteEnvironment(mode, processEnv = process.env) {
  const profileMatch = /^(standalone|cloud)\.(development|test|staging|production)$/u.exec(mode ?? '');
  return profileMatch?.[2]
    ?? (['development', 'test', 'staging', 'production'].includes(processEnv.SDKWORK_ENVIRONMENT ?? '')
      ? processEnv.SDKWORK_ENVIRONMENT
      : 'production');
}
import path from "node:path";
import { fileURLToPath } from "node:url";
import react from "@vitejs/plugin-react";
import { defineConfig, loadEnv } from "vite";

const appRoot = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, appRoot, "");
  return {
    build: {
      outDir: resolveBrowserDistOutDir(resolveViteEnvironment(mode, process.env)),
      emptyOutDir: true,
    },
    define: {
      "process.env.SDKWORK_ACCESS_TOKEN": JSON.stringify(env.SDKWORK_ACCESS_TOKEN ?? ""),
    },
    plugins: [react()],
    server: {
      port: 5182,
      host: "127.0.0.1",
    },
  };
});
