// @api/DataStore: Vencord's IndexedDB key-value store, kept by Rustcord in a file.
const data = globalThis.__fcData ??= {};
const stores = {};
const persist = () => __out({ op: "data", value: data });
const bucket = store => store ? (stores[store] ??= (data[`#${store}`] ??= {})) : data;

export const createStore = (db, name) => `${db}/${name}`;
export const get = async (k, s) => bucket(s)[k];
export const getMany = async (ks, s) => ks.map(k => bucket(s)[k]);
export const keys = async s => Object.keys(bucket(s)).filter(k => !k.startsWith("#"));
export const values = async s => (await keys(s)).map(k => bucket(s)[k]);
export const entries = async s => (await keys(s)).map(k => [k, bucket(s)[k]]);
export async function set(k, v, s) { bucket(s)[k] = v; persist(); }
export async function setMany(kv, s) { for (const [k, v] of kv) bucket(s)[k] = v; persist(); }
export async function update(k, f, s) { bucket(s)[k] = f(bucket(s)[k]); persist(); }
export async function del(k, s) { delete bucket(s)[k]; persist(); }
export async function delMany(ks, s) { for (const k of ks) delete bucket(s)[k]; persist(); }
export async function clear(s) { for (const k of await keys(s)) delete bucket(s)[k]; persist(); }
