import { createRequire } from "node:module";
import { pathToFileURL } from "node:url";

const require = createRequire(pathToFileURL(process.cwd() + "/"));
const chart = require("./tools/authoring_chart.js");

function assert(condition, message) {
  if (!condition) {
    console.error(message);
    process.exit(1);
  }
}

const tenQty1 = chart.dropExpectation(1000, 1, 1, 1000);
assert(tenQty1.successes === 100 && tenQty1.units === 100, JSON.stringify(tenQty1));

const tenQtyRange = chart.dropExpectation(1000, 1, 3, 1000);
assert(tenQtyRange.successes === 100 && tenQtyRange.units === 200, JSON.stringify(tenQtyRange));

const alwaysTwo = chart.dropExpectation(10000, 2, 2, 100);
assert(alwaysTwo.successes === 100 && alwaysTwo.units === 200, JSON.stringify(alwaysTwo));

const never = chart.dropExpectation(0, 1, 5, 1000);
assert(never.successes === 0 && never.units === 0, JSON.stringify(never));

const rare = chart.dropExpectation(1, 1, 2, 1);
assert(Math.abs(rare.units - 0.00015) < 1e-12, JSON.stringify(rare));
assert(chart.formatExact(rare.units) === "0.00015", chart.formatExact(rare.units));

const tagged = chart.filterItems(
  [
    { content_id: 1, label: "item.a", display_name: "A", category: "material", tags: ["quest"] },
    { content_id: 2, label: "item.b", display_name: "B", category: "tool", tags: ["scrap"] },
  ],
  "quest",
  "",
);
assert(tagged.length === 1 && tagged[0].content_id === 1, JSON.stringify(tagged));
const tools = chart.filterItems(
  [
    { content_id: 1, label: "item.a", display_name: "A", category: "material", tags: [] },
    { content_id: 2, label: "item.b", display_name: "B", category: "tool", tags: [] },
  ],
  "",
  "tool",
);
assert(tools.length === 1 && tools[0].content_id === 2, JSON.stringify(tools));

console.log("authoring chart ok");
