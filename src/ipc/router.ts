// Renderer-side IPC route table.
//
// Mirrors the former `electron/core/IpcRouter.ts`. The `path` strings are
// kept identical so all existing view/store code keeps working; each path
// maps to a Tauri command name in the Rust backend.

export type IpcRouter = {
  path: string;
  command: string;
};

export type Listener = {
  channel: string;
};

export const ipcRouters = {
  SERVER: {
    saveConfig: { path: "server/saveConfig", command: "server_save_config" },
    getServerConfig: {
      path: "server/getServerConfig",
      command: "server_get_server_config"
    },
    resetAllConfig: {
      path: "server/resetAllConfig",
      command: "server_reset_all_config"
    },
    exportConfig: {
      path: "server/exportConfig",
      command: "server_export_config"
    },
    importTomlConfig: {
      path: "server/importTomlConfig",
      command: "server_import_toml_config"
    },
    getLanguage: { path: "server/getLanguage", command: "server_get_language" },
    saveLanguage: {
      path: "server/saveLanguage",
      command: "server_save_language"
    }
  },
  LOG: {
    getFrpLogContent: {
      path: "log/getFrpLogContent",
      command: "log_get_frp_log_content"
    },
    getAppLogContent: {
      path: "log/getAppLogContent",
      command: "log_get_app_log_content"
    },
    openFrpcLogFile: {
      path: "log/openFrpcLogFile",
      command: "log_open_frpc_log_file"
    },
    openAppLogFile: {
      path: "log/openAppLogFile",
      command: "log_open_app_log_file"
    }
  },
  VERSION: {
    getVersions: {
      path: "version/getVersions",
      command: "version_get_versions"
    },
    downloadVersion: {
      path: "version/downloadVersion",
      command: "version_download_version"
    },
    getDownloadedVersions: {
      path: "version/getDownloadedVersions",
      command: "version_get_downloaded_versions"
    },
    deleteDownloadedVersion: {
      path: "version/deleteDownloadedVersion",
      command: "version_delete_downloaded_version"
    },
    importLocalFrpcVersion: {
      path: "version/importLocalFrpcVersion",
      command: "version_import_local_frpc_version"
    }
  },
  LAUNCH: {
    launch: { path: "launch/launch", command: "launch_launch" },
    terminate: { path: "launch/terminate", command: "launch_terminate" },
    getStatus: { path: "launch/getStatus", command: "launch_get_status" }
  },
  PROXY: {
    createProxy: { path: "proxy/createProxy", command: "proxy_create_proxy" },
    modifyProxy: { path: "proxy/modifyProxy", command: "proxy_modify_proxy" },
    deleteProxy: { path: "proxy/deleteProxy", command: "proxy_delete_proxy" },
    getAllProxies: {
      path: "proxy/getAllProxies",
      command: "proxy_get_all_proxies"
    },
    modifyProxyStatus: {
      path: "proxy/modifyProxyStatus",
      command: "proxy_modify_proxy_status"
    },
    getLocalPorts: {
      path: "proxy/getLocalPorts",
      command: "proxy_get_local_ports"
    }
  },
  SYSTEM: {
    openUrl: { path: "system/openUrl", command: "system_open_url" },
    relaunchApp: { path: "system/relaunchApp", command: "system_relaunch_app" },
    openAppData: {
      path: "system/openAppData",
      command: "system_open_app_data"
    },
    selectLocalFile: {
      path: "system/selectLocalFile",
      command: "system_select_local_file"
    },
    getFrpcDesktopGithubLastRelease: {
      path: "system/getFrpcDesktopGithubLastRelease",
      command: "system_get_frpc_desktop_github_last_release"
    }
  }
} as const;

export const listeners = {
  watchFrpcProcess: {
    channel: "frpcProcess:watchFrpcLog"
  },
  watchSystemUsage: {
    channel: "system:watchSystemUsage"
  }
} as const;
