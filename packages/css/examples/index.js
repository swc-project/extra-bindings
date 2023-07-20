const swc = require("../index.js");

const css = `
.foo {
  color: #FFFFFF;
  margin: 20px;
  margin-bottom: 10px;
}
`;

async function main() {
  console.time("🚀 minify Time");
  const output = await swc.minify(Buffer.from(css), {});
  console.timeEnd("🚀 minify Time");
  console.log(output.code.length + " bytes");
  console.log(output.code, "\n");
}
main();

console.time("🚀 minifySync Time");
const outputSync = swc.minifySync(Buffer.from(css), {});
console.timeEnd("🚀 minifySync Time");
console.log(outputSync.code.length + " bytes");
console.log(outputSync.code, "\n");
