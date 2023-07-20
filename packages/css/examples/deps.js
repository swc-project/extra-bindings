const swc = require("../index.js");

const css = `
.foo {
  color: #FFFFFF;
  margin: 20px;
  margin-bottom: 10px;
}
`;

async function main() {
  const output = await swc.transform(Buffer.from(css), {
    analyzeDependencies: true
  });
  console.log(output.deps);
}
main();
