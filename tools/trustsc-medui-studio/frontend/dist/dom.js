// The one tiny DOM-building helper shared by app.ts (toolbar, palette) and inspector.ts — this
// frontend deliberately has no framework (wave S9), so element construction stays this explicit.
export function el(tag, attrs = {}, children = []) {
    const node = document.createElement(tag);
    for (const [key, value] of Object.entries(attrs)) {
        node.setAttribute(key, value);
    }
    for (const child of children) {
        node.append(child);
    }
    return node;
}
