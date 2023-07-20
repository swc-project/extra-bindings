const swc = require("../index.js");

const css = `
.foo {
  color: #FFFFFF;
  margin: 20px;
  margin-bottom: 10px;
}
`;

async function main() {
  const output = swc.transformSync(Buffer.from(css), {
    analyzeDependencies: true
  });
  console.log(output.deps);
}
main();
