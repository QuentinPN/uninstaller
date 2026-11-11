/** A command whose call itself threw: reported as its own `failed` error. */
export const failedCall = () => ({ status: "error", error: "failed" }) as const;
