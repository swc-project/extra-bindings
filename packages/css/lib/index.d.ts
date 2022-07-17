/// <reference types="node" />
import binding = require("./binding");
export declare function minify(
  content: Buffer,
  options: any
): Promise<binding.TransformOutput>;
