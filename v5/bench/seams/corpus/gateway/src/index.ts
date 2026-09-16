// TypeScript gateway. `PaymentClient` here is the same conceptual type as the
// Rust struct and the Kotlin class — the cross-language seam a discrete query
// stitches by name.
export interface PaymentClient {
  charge(amount: number): boolean;
}

export class Gateway {
  client: PaymentClient;
}
