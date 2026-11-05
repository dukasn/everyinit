export type TraceLevel = "off" | "messages" | "verbose";

export interface EveryinitConfig {
  evinEnable: boolean;
  evinPath: string;
  eshEnable: boolean;
  eshPath: string;
  trace: TraceLevel;
}

export type SupportedLanguage = "flow" | "esh";

