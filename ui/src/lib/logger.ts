type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

const loggerPrefix = "[voltlane-ui]";

function write(level: LogLevel, message: string, meta?: unknown): void {
  const timestamp = new Date().toISOString();
  const payload = `${loggerPrefix} ${timestamp} ${message}`;
  const withMeta = meta === undefined ? [payload] : [payload, meta];

  switch (level) {
    case "trace":
      console.trace(...withMeta);
      break;
    case "debug":
      console.debug(...withMeta);
      break;
    case "info":
      console.info(...withMeta);
      break;
    case "warn":
      console.warn(...withMeta);
      break;
    case "error":
      console.error(...withMeta);
      break;
  }
}

export const logger = {
  trace: (message: string, meta?: unknown): void => write("trace", message, meta),
  debug: (message: string, meta?: unknown): void => write("debug", message, meta),
  info: (message: string, meta?: unknown): void => write("info", message, meta),
  warn: (message: string, meta?: unknown): void => write("warn", message, meta),
  error: (message: string, meta?: unknown): void => write("error", message, meta)
};
