import * as binding from "./binding";

type MinifierType = "js-module" | "js-script" | "json" | "css" | "html";

type Options = {
  filename?: string;
  iframeSrcdoc?: boolean;
  scriptingEnabled?: boolean;
  forceSetHtml5Doctype?: boolean;
  collapseWhitespaces?:
    | "none"
    | "all"
    | "smart"
    | "conservative"
    | "advanced-conservative"
    | "only-metadata";
  removeEmptyMetadataElements?: boolean;
  removeComments?: boolean;
  preserveComments: string[];
  minifyConditionalComments?: boolean;
  removeEmptyAttributes?: boolean;
  removeRedundantAttributes?: boolean;
  collapseBooleanAttributes?: boolean;
  normalizeAttributes?: boolean;
  minifyJson?: boolean | { pretty?: boolean };
  // TODO improve me after typing `@swc/css`
  minifyJs?: boolean | { parser?: any; minifier?: any; codegen?: any };
  minifyCss?: boolean | { parser?: any; minifier?: any; codegen?: any };
  minifyAdditionalScriptsContent?: [string, MinifierType][];
  minifyAdditionalAttributes?: [string, MinifierType][];
  sortSpaceSeparatedAttributeValues?: boolean;
  sortAttributes?: boolean;
  tagOmission?: boolean;
  selfClosingVoidElements?: boolean;
};

export async function minify(
  content: Buffer,
  options?: Options
): Promise<string> {
  return binding.minify(content, toBuffer(options ?? {}));
}

export function minifySync(content: Buffer, options?: Options) {
  return binding.minifySync(content, toBuffer(options ?? {}));
}

function toBuffer(t: any): Buffer {
  return Buffer.from(JSON.stringify(t));
}
