interface ApiResponse<T> {
  bizCode: string;
  data: T;
  message: string;
}

interface ControllerParam {
  // win: BrowserWindow;
  channel: string;
  args: any;
}

interface ListenerParam {
  // win: BrowserWindow;
  channel: string;
  args: any[];
}

type Listener = {
  channel: string;
};

enum IpcRouterKeys {
  SERVER = "SERVER",
  LOG = "LOG",
  VERSION = "VERSION",
  LAUNCH = "LAUNCH",
  PROXY = "PROXY",
  SYSTEM = "SYSTEM",
}
