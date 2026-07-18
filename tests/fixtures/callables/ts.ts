// Fixture: every EMITTED TS/JS callable kind for examples/callable-coverage.dl.
// free fn + const-bound arrow + function expr -> function; class members + ctor
// -> method; unbound arrow / function-expression argument -> lambda.

export function freeFunction(seed: number): number {
    return seed + 1;
}

// const-bound arrow: existing identity, call_def kind "function"
export const boundArrow = (factor: number): number => factor * 2;

// const-bound function expression: call_def kind "function"
export const boundFnExpr = function (offset: number): number {
    return offset - 1;
};

export async function asyncFree(payload: number): Promise<number> {
    return payload;
}

export function* generatorFn(limit: number): Generator<number> {
    for (let index = 0; index < limit; index++) {
        yield index;
    }
}

export class Widget {
    private size: number;

    // constructor -> method (sym file::method::Widget.constructor, name "Widget")
    constructor(size: number) {
        this.size = size;
    }

    // instance method -> method
    area(): number {
        return this.size * this.size;
    }

    // static method -> method
    static unit(): Widget {
        return new Widget(1);
    }

    // getter / setter -> method (share one sym)
    get width(): number {
        return this.size;
    }
    set width(value: number) {
        this.size = value;
    }
}

// unbound arrow passed as an argument -> lambda
export function useCallbacks(values: number[]): number {
    const doubled = values.map((value) => value * 2);
    // unbound function expression argument -> lambda
    return doubled.reduce(function (acc, value) {
        return acc + value;
    }, 0);
}
