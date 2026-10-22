export type Unlisten = () => void;
export interface Transport {
  call<T>(cmd: string, args?: Record<string, unknown>): Promise<T>;
  listen<T>(event: string, cb: (event: { payload: T }) => void): Promise<Unlisten>;
}
