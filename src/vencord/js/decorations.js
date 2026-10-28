// @api/MessageAccessories, @api/MessageDecorations and @api/MemberListDecorators: what plugins
// draw under a message, beside its author's name and beside a member. One registry for the
// three paths, since each loads this file as its own module.
const reg = globalThis.__fcDecorations ??= { accessories: new Map(), decorations: new Map(), members: new Map() };

export const accessories = reg.accessories;
export const addMessageAccessory = (id, render, position) => accessories.set(id, { render, position });
export const removeMessageAccessory = id => accessories.delete(id);

export const decorationsFactories = reg.decorations;
export const addMessageDecoration = (id, render) => decorationsFactories.set(id, render);
export const removeMessageDecoration = id => decorationsFactories.delete(id);

export const decoratorsFactories = reg.members;
export const addMemberListDecorator = (id, render, onlyIn) => decoratorsFactories.set(id, { render, onlyIn });
export const removeMemberListDecorator = id => decoratorsFactories.delete(id);
