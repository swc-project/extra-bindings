import * as binding from "./binding";

export type MinifyOptions = {
  filename?: string;
  sourceMap?: boolean;
};

export type TransformOptions = {
  filename?: string;

  sourceMap?: boolean

  cssModules?: CssModuleTransformOptions

  minify?: boolean

  /**
   * If true, swc will analyze dependencies of css files.
   */
  analyzeDependencies?: boolean
}

export type CssModuleTransformOptions = {
  pattern: String,
}

export async function minify(
  content: Buffer,
  options: MinifyOptions
): Promise<binding.TransformOutput> {
  return binding.minify(content, toBuffer(options ?? {}));
}

export function minifySync(content: Buffer, options: MinifyOptions) {
  return binding.minifySync(content, toBuffer(options ?? {}));
}

export async function transform(
  content: Buffer,
  options: TransformOptions
): Promise<binding.TransformOutput> {
  return binding.transform(content, toBuffer(options ?? {}));
}

export function transformSync(content: Buffer, options: TransformOptions) {
  return binding.transformSync(content, toBuffer(options ?? {}));
}

function toBuffer(t: any): Buffer {
  return Buffer.from(JSON.stringify(t));
}
