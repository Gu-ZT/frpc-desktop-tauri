import { defineConfig } from "vite";
import vue from "@vitejs/plugin-vue";
import { resolve } from "path";

/** 路径查找 */
const pathResolve = (dir: string): string => {
  return resolve(__dirname, ".", dir);
};

// https://vitejs.dev/config/
export default defineConfig(() => {
  return {
    css: {
      preprocessorOptions: {
        scss: {
          api: "modern-compiler"
        }
      } as any
    },
    plugins: [vue()],
    resolve: {
      alias: {
        "@": pathResolve("src")
      }
    },
    clearScreen: false,
    server: {
      host: "127.0.0.1",
      port: 3344,
      strictPort: true
    }
  };
});
