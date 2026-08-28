import { cmd } from "./client";
import { paymentApi } from "./payment";

jest.mock("./client", () => ({
  cmd: jest.fn().mockResolvedValue(undefined),
}));

describe("payment API compatibility contract", () => {
  it("retains the backend command shapes while billing is disabled", async () => {
    await paymentApi.confirmPayment("payment-key", "order-1", 1000);
    await paymentApi.getSubscriptionDetails();
    await paymentApi.cancelSubscription();
    await paymentApi.openPaymentPage();
    await paymentApi.openPaymentPage("YEARLY");

    expect((cmd as jest.Mock).mock.calls).toEqual([
      [
        "confirm_payment",
        { payment_key: "payment-key", order_id: "order-1", amount: 1000 },
      ],
      ["get_subscription_details"],
      ["cancel_subscription"],
      ["open_payment_page", { period: "MONTHLY" }],
      ["open_payment_page", { period: "YEARLY" }],
    ]);
  });
});
