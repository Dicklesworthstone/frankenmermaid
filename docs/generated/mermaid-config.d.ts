// GENERATED from mermaid_config_schema() (fm-core) — do not edit by hand.
// Regenerate: fm-cli config-schema --typescript <path>

export interface MermaidConfig {
  flowchart?: FlowchartConfig;
  gantt?: GanttConfig;
  /** Case-insensitive sanitization level: strict, antiscript, or loose. */
  securityLevel?: "strict" | "antiscript" | "loose";
  sequence?: SequenceConfig;
  /** Accepted for compatibility; currently has no runtime effect */
  startOnLoad?: boolean;
  theme?: string;
  themeVariables?: Record<string, string | number | boolean>;
}

export interface SequenceConfig {
  mirrorActors?: boolean;
  showSequenceNumbers?: boolean;
}

export interface GanttConfig {
  topAxis?: boolean;
}

export interface FlowchartConfig {
  /** Edge curve style, e.g. basis, linear, natural. */
  curve?: string;
  /** Case-insensitive layout direction: LR, RL, TB, TD, or BT. */
  direction?: "lr" | "rl" | "tb" | "td" | "bt";
  nodeSpacing?: number;
  /** Case-insensitive rank direction: LR, RL, TB, TD, or BT. Alias of direction. */
  rankDir?: "lr" | "rl" | "tb" | "td" | "bt";
  rankSpacing?: number;
}

/** One-line Mermaid directive: %%{constraints: <object>}%%. */
export type ConstraintsDirective = string;

/** One-line Mermaid directive: %%{init: <config-object>}%%. The payload must validate against this root schema. */
export type InitDirective = string;

