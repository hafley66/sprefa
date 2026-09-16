export interface Handle { release(): Promise<void> }
export function context$(): Promise<Handle> { return Promise.resolve({ release: async () => {} }) }
export function page$(): Promise<Handle> { return Promise.resolve({ release: async () => {} }) }
export async function acquire(src: Promise<Handle>): Promise<Handle> { return src }
