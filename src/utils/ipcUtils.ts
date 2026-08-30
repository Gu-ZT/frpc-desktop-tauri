// Renderer IPC helpers (Tauri port of the former Electron ipcUtils.ts).
//
// The public API is kept identical (`send`, `on`, `onListener`,
// `removeRouterListeners`) so views and stores only needed import changes.
//
// Semantics preserved:
// - every command returns `ApiResponse { bizCode, data, message }`
// - `bizCode === "A1000"` is success
// - `on(router, handler, errHandler)` subscribes to the single reply of one
//   request; the reply arrives asynchronously after `send()`.
// - `onListener(listener, handler)` subscribes to a push event channel.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { ElMessage } from "element-plus";

type ApiResponse = {
  bizCode: string;
  data: any;
  message: string;
};

const routerListeners = new Map<string, Set<(data: any) => void>>();
const routerErrListeners = new Map<
  string,
  Set<(bizCode: string, message: string) => void>
>();
const channelUnlisteners = new Map<string, UnlistenFn[]>();

export const on = (
  router: { command: string; path: string },
  listerHandler: (data: any) => void,
  errHandler?: (bizCode: string, message: string) => void
) => {
  if (!routerListeners.has(router.path)) {
    routerListeners.set(router.path, new Set());
  }
  routerListeners.get(router.path)!.add(listerHandler);
  if (errHandler) {
    if (!routerErrListeners.has(router.path)) {
      routerErrListeners.set(router.path, new Set());
    }
    routerErrListeners.get(router.path)!.add(errHandler);
  }
};

export const onListener = (
  listener: { channel: string },
  listerHandler: (data: any) => void
) => {
  if (!channelUnlisteners.has(listener.channel)) {
    channelUnlisteners.set(listener.channel, []);
  }
  const unlisteners = channelUnlisteners.get(listener.channel)!;
  if (unlisteners.length === 0) {
    listen<ApiResponse | any>(listener.channel, event => {
      const payload = event.payload;
      // Push events carry either a bare payload or an ApiResponse wrapper.
      const data =
        payload && typeof payload === "object" && "bizCode" in payload
          ? payload.bizCode === "A1000"
            ? payload.data
            : undefined
          : payload;
      if (data !== undefined) {
        const handlers = channelHandlers.get(listener.channel);
        handlers?.forEach(handler => handler(data));
      }
    }).then(unlisten => {
      unlisteners.push(unlisten);
    });
  }
  if (!channelHandlers.has(listener.channel)) {
    channelHandlers.set(listener.channel, new Set());
  }
  channelHandlers.get(listener.channel)!.add(listerHandler);
};

const channelHandlers = new Map<string, Set<(data: any) => void>>();

export const removeRouterListeners = (router: {
  command: string;
  path: string;
}) => {
  routerListeners.delete(router.path);
  routerErrListeners.delete(router.path);
};

export const removeRouterListeners2 = (listen: { channel: string }) => {
  const unlisteners = channelUnlisteners.get(listen.channel);
  unlisteners?.forEach(unlisten => unlisten());
  channelUnlisteners.delete(listen.channel);
  channelHandlers.delete(listen.channel);
};

// ---------------------------------------------------------------------------
// Download progress bridging
// ---------------------------------------------------------------------------
//
// The Rust backend pushes download progress on the `version:downloadProgress`
// event channel; the former Electron code delivered it as a reply on the
// `version/downloadVersion` router path. We bridge the two so the download
// view keeps working unchanged.

let progressBridged = false;

function bridgeDownloadProgress() {
  if (progressBridged) {
    return;
  }
  progressBridged = true;
  listen<{ percent: number; githubReleaseId: number; completed: boolean }>(
    "version:downloadProgress",
    event => {
      const data = event.payload;
      const handlers = routerListeners.get("version/downloadVersion");
      handlers?.forEach(handler => handler(data));
    }
  ).catch(() => {
    // channel not available (e.g. plain browser dev); ignore
  });
}

export const send = (
  router: { command: string; path: string },
  params?: any
) => {
  if (router.path === "version/downloadVersion") {
    bridgeDownloadProgress();
  }
  invoke<ApiResponse>(router.command, { args: params ?? {} })
    .then(resp => {
      const { bizCode, data, message } = resp;
      if (bizCode === "A1000") {
        const handlers = routerListeners.get(router.path);
        handlers?.forEach(handler => handler(data));
      } else {
        const errHandlers = routerErrListeners.get(router.path);
        if (errHandlers && errHandlers.size > 0) {
          errHandlers.forEach(errHandler => errHandler(bizCode, message));
        } else {
          ElMessage({
            message: message || "internal error.",
            type: "error"
          });
        }
      }
    })
    .catch(err => {
      const errHandlers = routerErrListeners.get(router.path);
      if (errHandlers && errHandlers.size > 0) {
        errHandlers.forEach(errHandler => errHandler("B1000", String(err)));
      } else {
        ElMessage({
          message: String(err),
          type: "error"
        });
      }
    });
};
