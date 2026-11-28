// One key-value store in IndexedDB. The names are idb-keyval's, which
// this replaces, so a device keeps what it saved before.
const DB = "keyval-store";
const STORE = "keyval";

let db: Promise<IDBDatabase> | undefined;

function open(): Promise<IDBDatabase> {
  db ??= new Promise((resolve, reject) => {
    const req = indexedDB.open(DB);
    req.onupgradeneeded = () => req.result.createObjectStore(STORE);
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => {
      db = undefined;
      reject(req.error);
    };
  });
  return db;
}

function request<T>(
  mode: IDBTransactionMode,
  op: (s: IDBObjectStore) => IDBRequest<T>,
): Promise<T> {
  return open().then(
    (d) =>
      new Promise<T>((resolve, reject) => {
        const tx = d.transaction(STORE, mode);
        const req = op(tx.objectStore(STORE));
        tx.oncomplete = () => resolve(req.result);
        tx.onabort = tx.onerror = () => reject(tx.error ?? req.error);
      }),
  );
}

export function idbGet<T = unknown>(key: IDBValidKey): Promise<T | undefined> {
  return request("readonly", (s) => s.get(key));
}

export function idbSet(key: IDBValidKey, value: unknown): Promise<void> {
  return request("readwrite", (s) => s.put(value, key)).then(() => undefined);
}
