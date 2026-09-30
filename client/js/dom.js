// DOM helper (UI_SPEC §2.3). Never uses innerHTML.
//
// el(tag, attrs, ...children)
//   attrs:
//     onClick / onclick / onInput …  → addEventListener("click" / "input" …)
//     value, checked                 → set as DOM properties (live form state)
//     anything else                  → setAttribute (class, disabled, title, id, …)
//     null / undefined / false       → skipped (so `disabled: !canPlay` works)
//     true                           → boolean attribute (setAttribute(k, ""))
//   children: strings / numbers become text nodes; Nodes are appended;
//     arrays are flattened; null / undefined / false are skipped.

export function el(tag, attrs, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(attrs || {})) {
    if (value === null || value === undefined || value === false) continue;
    if (key.length > 2 && key.startsWith("on") && typeof value === "function") {
      node.addEventListener(key.slice(2).toLowerCase(), value);
    } else if (key === "value" || key === "checked") {
      node[key] = value;
    } else {
      node.setAttribute(key, value === true ? "" : String(value));
    }
  }
  append(node, children);
  return node;
}

function append(node, children) {
  for (const child of children) {
    if (child === null || child === undefined || child === false) continue;
    if (Array.isArray(child)) append(node, child);
    else if (child instanceof Node) node.appendChild(child);
    else node.appendChild(document.createTextNode(String(child)));
  }
}
