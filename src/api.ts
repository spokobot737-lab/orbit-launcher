import { invoke, convertFileSrc, isTauri } from "@tauri-apps/api/core";
export const native = isTauri();
export function api<T = any>(op: string, args: unknown = {}): Promise<T> {
  if (!native)
    return Promise.reject(
      new Error(
        "Open the Windows desktop application to use local storage and native services. This browser view is a UI preview; no mock integrations are active.",
      ),
    );
  return invoke<T>("api", { op, args });
}
export type Item = {
  id: string;
  kind: "minecraft" | "roblox" | "program";
  name: string;
  favorite?: boolean;
  lastLaunch?: number;
  image?: string;
  path?: string;
  args?: string[];
  workingDir?: string;
  gameActivity?: boolean;
  version?: string;
  loader?: string;
  memory?: number;
  width?: number;
  height?: number;
  javaArgs?: string[];
  account?: string;
  offlineName?: string;
  fullscreen?: boolean;
  performance?: boolean;
  universeId?: number;
  rootPlaceId?: number;
};
export type Collection = { id: string; name: string; items: string[] };
export const errorText = (e: unknown) =>
  e instanceof Error ? e.message : String(e);
const images = new Map<string, Promise<string>>();
export function imageUrl(url: string): Promise<string> {
  if (url.startsWith("data:image/")) return Promise.resolve(url);
  if (!images.has(url))
    images.set(
      url,
      api<string>("image", { url })
        .then(convertFileSrc)
        .catch((e) => {
          images.delete(url);
          throw e;
        }),
    );
  return images.get(url)!;
}
