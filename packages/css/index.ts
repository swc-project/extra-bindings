import * as binding from "./binding";

export type Options = {
  filename?: string;
  sourceMap?: boolean;
};

export async function minify(
  content: Buffer,
  options: Options
): Promise<binding.TransformOutput> {
  return binding.minify(content, toBuffer(options ?? {}));
}

export function minifySync(content: Buffer, options: Options) {
  return binding.minifySync(content, toBuffer(options ?? {}));
}

function toBuffer(t: any): Buffer {
  return Buffer.from(JSON.stringify(t));
}
