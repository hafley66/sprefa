function leaf(n: number): number {
    return n + 1;
}

// An object literal whose members are arrows: the tsc oracle names the
// enclosing callable by the property (`getAllCodeActions`), and the arrow's
// call sites must name it as their caller too.
const handlers = {
    getAllCodeActions: context => leaf(context),
};

export function run() {
    handlers.getAllCodeActions(1);
}
