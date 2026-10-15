// Svelte action: moves an element to <body> so fixed positioning isn't affected
// by transformed / contained ancestors (e.g. the virtual grid).
export function portal(node: HTMLElement) {
  document.body.appendChild(node);
  return {
    destroy() {
      node.remove();
    },
  };
}
