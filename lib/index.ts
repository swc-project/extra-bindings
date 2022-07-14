import binding = require("./binding");

export async function minify(
  content: Buffer,
  options: any
): Promise<binding.TransformOutput> {
  return binding.minify(content, toBuffer(options));
}

function toBuffer(t: any): Buffer {
  return Buffer.from(JSON.stringify(t));
}
