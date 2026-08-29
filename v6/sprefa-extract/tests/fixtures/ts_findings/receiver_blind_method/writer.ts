// Half of the `receiver_blind_method` finding. See `consumer.ts` for the header.
export class Writer {
    lines: string[] = [];

    push(): void {
        this.lines[this.lines.length] = "";
    }
}
