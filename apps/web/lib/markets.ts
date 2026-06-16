export type MarketStatus = "open" | "resolved" | "expired";

export interface Market {
  id: string;
  question: string;
  yesPrice: number;
  noPrice: number;
  volume: string;
  status: MarketStatus;
  endsAt: string;
}

export const MOCK_MARKETS: Market[] = [
  {
    id: "mkt-1",
    question: "Will BTC close above $100k on Dec 31, 2026?",
    yesPrice: 0.62,
    noPrice: 0.38,
    volume: "12,400 XLM",
    status: "open",
    endsAt: "2026-12-31",
  },
  {
    id: "mkt-2",
    question: "Will the Fed cut rates before Q3 2026?",
    yesPrice: 0.41,
    noPrice: 0.59,
    volume: "8,200 XLM",
    status: "open",
    endsAt: "2026-09-30",
  },
  {
    id: "mkt-3",
    question: "Will Stellar process 1B+ ops in 2026?",
    yesPrice: 0.55,
    noPrice: 0.45,
    volume: "3,100 XLM",
    status: "open",
    endsAt: "2026-12-31",
  },
  {
    id: "mkt-4",
    question: "Demo market: protocol upgrade passes?",
    yesPrice: 0.78,
    noPrice: 0.22,
    volume: "1,050 XLM",
    status: "resolved",
    endsAt: "2025-06-01",
  },
];
