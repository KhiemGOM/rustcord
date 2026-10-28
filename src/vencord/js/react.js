// React as far as plugins use it to describe UI. Fastcord renders what the elements say
// natively, once per ask, so state starts where it's given and effects don't run.
export const Fragment = VencordFragment;
export const createElement = VencordCreateElement;
export const isValidElement = x => !!x?.$$vnode;
export const cloneElement = (el, props, ...children) => ({ ...el, props: { ...el.props, ...props, ...(children.length ? { children } : {}) } });
export const useState = init => [typeof init === "function" ? init() : init, () => {}];
export const useReducer = (r, init, f) => [f ? f(init) : init, () => {}];
export const useEffect = () => {};
export const useLayoutEffect = () => {};
export const useInsertionEffect = () => {};
export const useMemo = f => f();
export const useCallback = f => f;
export const useRef = v => ({ current: v });
export const useContext = c => c?._value;
export const useId = () => "fc" + Math.random().toString(36).slice(2);
export const useSyncExternalStore = (_, get) => get();
export const useTransition = () => [false, f => f()];
export const useDeferredValue = v => v;
export const useImperativeHandle = () => {};
export const useDebugValue = () => {};
export const createContext = v => ({ _value: v, Provider: p => p.children, Consumer: p => p.children?.(v) });
export const memo = c => c;
export const forwardRef = c => p => c(p, null);
export const lazy = load => p => null;
export const Suspense = p => p.children;
export const StrictMode = p => p.children;
export const createRef = () => ({ current: null });
export class Component {
    constructor(props) { this.props = props; this.state = {}; }
    setState() {}
    forceUpdate() {}
}
export class PureComponent extends Component {}
const flat = c => c == null ? [] : Array.isArray(c) ? c.flat(Infinity) : [c];
export const Children = {
    map: (c, f) => flat(c).map(f),
    forEach: (c, f) => flat(c).forEach(f),
    toArray: flat,
    count: c => flat(c).length,
    only: c => flat(c)[0],
};
const React = { Fragment, createElement, isValidElement, cloneElement, useState, useReducer, useEffect, useLayoutEffect, useInsertionEffect, useMemo, useCallback, useRef, useContext, useId, useSyncExternalStore, useTransition, useDeferredValue, useImperativeHandle, useDebugValue, createContext, memo, forwardRef, lazy, Suspense, StrictMode, createRef, Component, PureComponent, Children, version: "19.0.0" };
export default React;
