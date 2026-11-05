/**
 * Wraps each Markdown table in `.table-wrap`, which draws its edge and lets a
 * wide table scroll inside the column on a narrow screen.
 */
export function createTableWrapPlugin() {
  return {
    name: "guide-table-wrap",
    element: {
      filter: ["table"],
      visit(node, ctx) {
        ctx.wrapNode(node, { type: "element", tagName: "div", properties: { className: ["table-wrap"] }, children: [] });
      },
    },
  };
}
