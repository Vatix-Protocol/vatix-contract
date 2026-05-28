import type { Metadata } from "next";
import { Geist } from "next/font/google";
import "./globals.css";
import { Navbar } from "@/components/Navbar";
import { WalletProvider } from "@/context/WalletContext";
import { DarkModeErrorBoundary } from "@/components/DarkModeErrorBoundary";

const geistSans = Geist({
  variable: "--font-geist-sans",
  subsets: ["latin"],
});

export const metadata: Metadata = {
  title: "Vatix Markets",
  description: "Prediction markets on Stellar — Vatix Protocol",
};

export default function RootLayout({
  children,
}: Readonly<{
  children: React.ReactNode;
}>) {
  return (
    <html lang="en" className={`${geistSans.variable} h-full antialiased`}>
      <body className="min-h-full flex flex-col">
        <DarkModeErrorBoundary>
          <WalletProvider>
            <Navbar />
            <main className="flex-1">{children}</main>
          </WalletProvider>
        </DarkModeErrorBoundary>
      </body>
    </html>
  );
}
