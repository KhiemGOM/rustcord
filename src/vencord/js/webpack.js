// @webpack: plugins finding Discord's modules by their shape. Fastcord has no webpack, so a
// store asked for by name is Fastcord's when it has one, and anything else is a stub.
import * as common from "@webpack/common";

const named = name => common[name] && !common[name].__fcStub ? common[name] : __stub(`webpack:${name}`);
const any = what => __stub(`webpack:${what}`);

export const wreq = any("wreq");
export const cache = {};
export const filters = new Proxy({}, { get: () => () => () => false });
export const find = () => undefined;
export const findAll = () => [];
export const findBulk = (...f) => f.map(() => undefined);
export const findByProps = (...props) => any(props.join(","));
export const findByPropsLazy = findByProps;
export const findAllByProps = () => [];
export const findByCode = (...code) => any(code.join(","));
export const findByCodeLazy = findByCode;
export const findAllByCode = () => [];
export const findComponentByCode = () => () => null;
export const findComponentByCodeLazy = findComponentByCode;
export const findExportedComponent = name => named(name);
export const findExportedComponentLazy = findExportedComponent;
export const findComponentLazy = () => () => null;
export const findStore = name => named(name);
export const findStoreLazy = findStore;
export const findLazy = () => any("lazy");
export const findModuleId = () => undefined;
export const findModuleFactory = () => undefined;
export const findCssClasses = () => new Proxy({}, { get: () => "" });
export const findCssClassesLazy = findCssClasses;
export const mapMangledModule = (_, map) => Object.fromEntries(Object.keys(map).map(k => [k, named(k)]));
export const mapMangledModuleLazy = mapMangledModule;
export const mapMangledCssClasses = (_, names) => Object.fromEntries(names.map(n => [n, ""]));
export const extractAndLoadChunks = async () => {};
export const extractAndLoadChunksLazy = () => async () => {};
export const waitFor = () => {};
export const onceReady = Promise.resolve();
export const search = () => ({});
export const extract = () => null;
export const proxyLazyWebpack = f => any("proxyLazyWebpack");
export const LazyComponentWebpack = () => () => null;
