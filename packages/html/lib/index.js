"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
exports.minifySync = exports.minify = void 0;
const binding = require("./binding");
async function minify(content, options) {
  return binding.minify(content, toBuffer(options));
}
exports.minify = minify;
function minifySync(content, options) {
  return binding.minifySync(content, toBuffer(options));
}
exports.minifySync = minifySync;
function toBuffer(t) {
  return Buffer.from(JSON.stringify(t));
}
