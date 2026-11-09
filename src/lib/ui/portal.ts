/** Moves the element to the end of `<body>`, outside the app's layout, and removes
 * it when its owner unmounts. */
export function portal(node: HTMLElement) {
  document.body.append(node);
  return { destroy: () => node.remove() };
}
