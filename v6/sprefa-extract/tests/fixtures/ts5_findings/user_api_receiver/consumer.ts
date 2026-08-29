// `program` is a local, so the receiver is unknown, but `getTypeChecker` is no
// ECMAScript member name and the corpus declares exactly one.
interface Program {
    getTypeChecker(): number;
}

export function run(program: Program): number {
    return program.getTypeChecker();
}
