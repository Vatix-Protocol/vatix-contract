import { render, screen } from "@testing-library/react";
import { WalletConnectButton } from "../WalletConnectButton";

describe("WalletConnectButton", () => {
  it("renders connect button when not connected", () => {
    render(
      <WalletConnectButton
        address={null}
        isConnecting={false}
        onConnect={() => {}}
        onDisconnect={() => {}}
      />
    );

    expect(screen.getByText("Connect wallet")).toBeInTheDocument();
  });
});
