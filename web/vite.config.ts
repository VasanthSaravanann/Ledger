import { defineConfig } from "vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import tailwindcss from "@tailwindcss/vite";
import tsConfigPaths from "vite-tsconfig-paths";
import { nitro } from "nitro/vite";

/**
 * Plain TanStack Start + Nitro. The app has exactly one deployment target, so
 * Nitro's default preset is `vercel`: `bun run build` writes `.vercel/output`
 * (Build Output API v3) with no environment variable set. `NITRO_PRESET` still
 * overrides it if you want to build for something else.
 */
export default defineConfig(({ command }) => ({
  plugins: [
    tailwindcss(),
    tsConfigPaths({ projects: ["./tsconfig.json"] }),
    tanstackStart({
      server: { entry: "server" },
      importProtection: {
        behavior: "error",
        client: { files: ["**/server/**"], specifiers: ["server-only"] },
      },
    }),
    ...(command === "build" ? [nitro({ defaultPreset: "vercel" })] : []),
    viteReact(),
  ],
}));
