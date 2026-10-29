/**
 * Creates a Stripe Checkout session for the amount picked on the L.O.B.A
 * download block, and returns its URL. No Stripe SDK: one form-encoded POST.
 *
 * Runs anywhere that speaks the web Request/Response API:
 *   Next.js (app router)  app/api/loba-checkout/route.ts → export { POST }
 *   Netlify Functions v2  netlify/functions/loba-checkout.ts → default export
 *   Cloudflare / Deno     call handle(request) from your fetch handler
 *
 * Environment:
 *   STRIPE_SECRET_KEY   sk_live_… (sk_test_… while trying it out)
 *   LOBA_SUCCESS_URL    where Stripe sends people after paying. A thank-you
 *                       page that starts the download is nicest; the DMG link
 *                       itself also works.
 *   LOBA_CANCEL_URL     back to the product page
 */

const MIN = 1; // Stripe won't charge less than about $0.50; free is "Skip"
const MAX = 500; // a typo guard, not a ceiling on generosity

export async function handle(request: Request): Promise<Response> {
  if (request.method !== "POST") {
    return json({ error: "POST only" }, 405);
  }

  let amount: number;
  try {
    amount = Number((await request.json()).amount);
  } catch {
    return json({ error: "Bad request" }, 400);
  }
  // Whole dollars only: the slider steps by 1, and anything else is tampering.
  if (!Number.isInteger(amount) || amount < MIN || amount > MAX) {
    return json({ error: `Pick a whole amount from $${MIN} to $${MAX}` }, 400);
  }

  const key = env("STRIPE_SECRET_KEY");
  const success = env("LOBA_SUCCESS_URL");
  const cancel = env("LOBA_CANCEL_URL");
  if (!key || !success || !cancel) {
    return json({ error: "Checkout isn't set up" }, 500);
  }

  const form = new URLSearchParams({
    mode: "payment",
    // Stripe's button then reads "Donate" instead of "Pay".
    submit_type: "donate",
    "line_items[0][quantity]": "1",
    "line_items[0][price_data][currency]": "usd",
    "line_items[0][price_data][unit_amount]": String(amount * 100),
    "line_items[0][price_data][product_data][name]": "L.O.B.A for Mac",
    "line_items[0][price_data][product_data][description]":
      "Pay what you want. Thank you.",
    success_url: success,
    cancel_url: cancel,
  });

  const res = await fetch("https://api.stripe.com/v1/checkout/sessions", {
    method: "POST",
    headers: {
      Authorization: `Bearer ${key}`,
      "Content-Type": "application/x-www-form-urlencoded",
    },
    body: form,
  });
  const session = (await res.json()) as { url?: string; error?: unknown };
  if (!res.ok || !session.url) {
    console.error("Stripe checkout failed", session.error);
    return json({ error: "Checkout didn't open" }, 502);
  }
  return json({ url: session.url }, 200);
}

export const POST = handle;
export default handle;

function json(body: unknown, status: number): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

function env(name: string): string | undefined {
  const g = globalThis as {
    process?: { env: Record<string, string | undefined> };
    Deno?: { env: { get(k: string): string | undefined } };
  };
  return g.process?.env[name] ?? g.Deno?.env.get(name);
}
