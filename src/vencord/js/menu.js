// @api/ContextMenu: plugins add items to a menu by its Discord nav id; Fastcord asks for them
// when it opens that menu and draws them under its own.
export const navPatches = new Map();
export const globalPatches = new Set();

export function addContextMenuPatch(navId, patch) {
    for (const id of [navId].flat()) {
        let set = navPatches.get(id);
        if (!set) navPatches.set(id, set = new Set());
        set.add(patch);
    }
}

export function removeContextMenuPatch(navId, patch) {
    const done = [navId].flat().map(id => navPatches.get(id)?.delete(patch) ?? false);
    return Array.isArray(navId) ? done : done[0];
}

export const addGlobalContextMenuPatch = patch => globalPatches.add(patch);
export const removeGlobalContextMenuPatch = patch => globalPatches.delete(patch);

export function findGroupChildrenByChildId(id, children, matchSubstring = false) {
    for (const child of children) {
        if (child == null) continue;
        if (Array.isArray(child)) {
            const found = findGroupChildrenByChildId(id, child, matchSubstring);
            if (found !== null) return found;
        }
        const is = x => matchSubstring ? child.props?.id?.includes(x) : child.props?.id === x;
        if ((Array.isArray(id) && id.some(is)) || is(id)) return children;
        let next = child.props?.children;
        if (next) {
            if (!Array.isArray(next)) child.props.children = next = [next];
            const found = findGroupChildrenByChildId(id, next, matchSubstring);
            if (found !== null) return found;
        }
    }
    return null;
}

/// The items plugins add to `navId`'s menu, given the props Discord would pass it.
export function build(navId, props) {
    const children = [];
    for (const patch of navPatches.get(navId) ?? []) {
        try { patch(children, props); } catch (e) { console.error(`context menu patch for ${navId} threw:`, e); }
    }
    for (const patch of globalPatches) {
        try { patch(navId, children, props); } catch (e) { console.error(`global context menu patch threw:`, e); }
    }
    return children;
}
