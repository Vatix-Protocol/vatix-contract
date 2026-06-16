/**
 * Generates 375 simple contributor issues (125 × 3 repos).
 * Default: write JSON under scripts/issues/generated/
 * --publish: create on GitHub via gh CLI (skips duplicate titles)
 */

import { execSync, spawnSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const OUT_DIR = join(__dirname, "issues", "generated");
const ISSUES_PER_REPO = 125;
const DEFAULT_LABELS = ["good first issue", "easy"] as const;
// Used only as a label so we can filter “extra” issues later.
// Titles for extra issues intentionally do NOT include this prefix.
const TOPUP_LABEL = "topup";

export interface GeneratedIssue {
  repo: string;
  github: string;
  number: number;
  title: string;
  body: string;
  labels: string[];
  area: string;
}

interface RepoConfig {
  slug: string;
  github: string;
  buckets: Bucket[];
}

interface Bucket {
  area: string;
  label: string;
  count: number;
  templates: TaskTemplate[];
}

interface TaskTemplate {
  title: (ctx: TaskContext) => string;
  body: (ctx: TaskContext) => string;
}

interface TaskContext {
  index: number;
  area: string;
  target: string;
}

const TARGETS: Record<string, string[]> = {
  web: [
    "Navbar",
    "MarketCard",
    "MarketsPage",
    "HomePage",
    "WalletContext",
    "markets list",
    "wallet connect button",
    "market detail route",
    "position panel",
    "deposit form",
    "withdraw form",
    "footer",
    "loading skeleton",
    "error banner",
    "empty state",
    "price bar",
    "status badge",
    "responsive layout",
    "dark mode styles",
    "metadata",
  ],
  contract: [
    "deposit module",
    "withdraw module",
    "positions module",
    "settlement module",
    "oracle module",
    "validation helpers",
    "storage layout",
    "event emissions",
    "error types",
    "market initialization",
    "integration test harness",
    "withdraw edge case",
    "position limits",
    "fee calculation",
    "expiration check",
  ],
  tooling: [
    "deploy.sh",
    "deploy-testnet.sh",
    "invoke-example.sh",
    "CI workflow",
    "README",
    "contract Makefile",
    "rustfmt config",
    "clippy lints",
  ],
  api: [
    "health route",
    "markets route",
    "orders route",
    "positions route",
    "Prisma schema",
    "migration validator",
    "env loader",
    "request context",
    "CORS config",
    "error handler",
    "OpenAPI stub",
    "rate limiter",
    "Redis client",
    "integration test setup",
    "docker compose",
  ],
  indexer: [
    "ledger cursor",
    "event parser",
    "retry logic",
    "batch writer",
    "metrics log",
    "startup health",
  ],
  oracle: [
    "price fetcher",
    "signature helper",
    "config loader",
    "submission queue",
    "timeout handler",
  ],
  workers: [
    "finalization job",
    "queue consumer",
    "graceful shutdown",
    "dead letter log",
  ],
  shared: [
    "logger",
    "config types",
    "env validation",
    "test utilities",
  ],
  swyftWeb: [
    "SwapWidget",
    "PoolList",
    "PortfolioPage",
    "LiquidityAdd",
    "TokenSelector",
    "PriceChart",
    "WalletContext",
    "slippage panel",
    "history table",
    "position remove flow",
  ],
  swyftApi: [
    "pools controller",
    "swaps service",
    "indexer module",
    "candles worker",
    "webhooks module",
    "auth nonce",
    "Prisma seed",
    "BullMQ queue",
    "Swagger tags",
    "rate limit guard",
  ],
  swyftSdk: [
    "quote helper",
    "liquidity math",
    "position queries",
    "swap builder",
    "tick math tests",
  ],
  swyftContract: [
    "pool contract",
    "router contract",
    "position NFT",
    "pool factory",
    "math-lib",
    "deploy script",
    "integration test",
  ],
};

function pick<T>(arr: T[], index: number): T {
  return arr[index % arr.length]!;
}

function task(
  pattern: string,
  acceptance: string,
): TaskTemplate {
  return {
    title: (ctx) => pattern.replace("{target}", ctx.target),
    body: (ctx) =>
      [
        `## Context`,
        `Small onboarding task for **${ctx.area}**.`,
        ``,
        `## Task`,
        pattern.replace("{target}", ctx.target),
        ``,
        `## Acceptance criteria`,
        acceptance.replace("{target}", ctx.target),
        ``,
        `## Notes`,
        `- Keep the PR focused on this task only`,
        `- Ask questions in the issue thread if scope is unclear`,
      ].join("\n"),
  };
}

const WEB_TASKS: TaskTemplate[] = [
  task("Add loading skeleton to {target}", "- [ ] Skeleton shows while data is loading\n- [ ] No layout shift when content appears"),
  task("Add empty state copy for {target}", "- [ ] Friendly message when list is empty\n- [ ] Link or CTA where appropriate"),
  task("Improve keyboard navigation on {target}", "- [ ] All interactive elements reachable via Tab\n- [ ] Visible focus ring"),
  task("Add aria-label to interactive {target}", "- [ ] Buttons/links have accessible names\n- [ ] Passes axe spot check"),
  task("Add unit test stub for {target}", "- [ ] Test file created under apps/web\n- [ ] One smoke assertion"),
  task("Document props for {target} component", "- [ ] JSDoc on exported props\n- [ ] Example usage in comment"),
  task("Extract helper used by {target} into lib/", "- [ ] Pure function with types\n- [ ] Imported from single place"),
  task("Add error boundary around {target}", "- [ ] Fallback UI on throw\n- [ ] Error logged to console in dev"),
  task("Polish responsive styles for {target}", "- [ ] Looks correct at 375px and 1280px\n- [ ] No horizontal scroll"),
  task("Add dark mode contrast fix for {target}", "- [ ] Text meets WCAG AA on dark background"),
];

const CONTRACT_TASKS: TaskTemplate[] = [
  task("Add unit test case for {target}", "- [ ] Test in contracts/market\n- [ ] `cargo test` passes"),
  task("Add doc comment to {target}", "- [ ] Public items documented\n- [ ] Example in comment where helpful"),
  task("Improve error message in {target}", "- [ ] Clear variant name\n- [ ] Message explains failure"),
  task("Add validation guard in {target}", "- [ ] Invalid input rejected\n- [ ] Test covers rejection"),
  task("Emit event for action in {target}", "- [ ] Event defined in events.rs\n- [ ] Test asserts emission"),
  task("Refactor {target} for readability", "- [ ] No behavior change\n- [ ] Clippy clean"),
  task("Add TODO with issue link near {target}", "- [ ] TODO references GitHub issue format\n- [ ] Short explanation"),
];

const TOOLING_TASKS: TaskTemplate[] = [
  task("Document usage of {target}", "- [ ] README section added\n- [ ] Example command included"),
  task("Add echo guard comment to {target}", "- [ ] Notes what real impl should do\n- [ ] Linked from scripts README"),
  task("Add CI comment explaining {target} step", "- [ ] Future maintainers understand purpose"),
];

const BACKEND_TASKS: TaskTemplate[] = [
  task("Add Vitest test for {target}", "- [ ] Test file colocated or under tests/\n- [ ] `pnpm test:run` passes"),
  task("Add TypeScript type for {target}", "- [ ] No `any` in new code\n- [ ] Exported where needed"),
  task("Improve log message in {target}", "- [ ] Structured fields\n- [ ] Appropriate log level"),
  task("Add input validation to {target}", "- [ ] 400 on invalid input\n- [ ] Test covers case"),
  task("Document {target} in docs/", "- [ ] Markdown section\n- [ ] Links from README if relevant"),
  task("Add env var to .env.example for {target}", "- [ ] Comment explains purpose\n- [ ] Optional vs required clear"),
];

const SWYFT_TASKS: TaskTemplate[] = [
  task("Add loading state to {target}", "- [ ] Spinner or skeleton\n- [ ] Disabled actions while loading"),
  task("Add test coverage for {target}", "- [ ] Jest or Vitest test\n- [ ] CI passes"),
  task("Improve TypeScript types in {target}", "- [ ] Strict types\n- [ ] No new eslint warnings"),
  task("Add JSDoc to exported API in {target}", "- [ ] Params documented\n- [ ] Return type described"),
  task("Handle empty data in {target}", "- [ ] UI does not break\n- [ ] Copy explains next step"),
];

function buildBuckets(
  areaKey: keyof typeof TARGETS,
  label: string,
  count: number,
  templates: TaskTemplate[],
): Bucket {
  return {
    area: areaKey,
    label,
    count,
    templates,
  };
}

const REPOS: RepoConfig[] = [
  {
    slug: "vatix-contract",
    github: "Vatix-Protocol/vatix-contract",
    buckets: [
      buildBuckets("web", "frontend", 60, WEB_TASKS),
      buildBuckets("contract", "contracts", 40, CONTRACT_TASKS),
      buildBuckets("tooling", "tooling", 25, TOOLING_TASKS),
    ],
  },
  {
    slug: "vatix-backend",
    github: "Vatix-Protocol/vatix-backend",
    buckets: [
      buildBuckets("api", "api", 55, BACKEND_TASKS),
      buildBuckets("indexer", "indexer", 20, BACKEND_TASKS),
      buildBuckets("oracle", "oracle", 15, BACKEND_TASKS),
      buildBuckets("workers", "workers", 15, BACKEND_TASKS),
      buildBuckets("shared", "shared", 20, BACKEND_TASKS),
    ],
  },
  {
    slug: "swyft",
    github: "Vatix-Protocol/Swyft",
    buckets: [
      buildBuckets("swyftWeb", "frontend", 45, SWYFT_TASKS),
      buildBuckets("swyftApi", "api", 45, SWYFT_TASKS),
      buildBuckets("swyftSdk", "sdk", 20, SWYFT_TASKS),
      buildBuckets("swyftContract", "contracts", 15, SWYFT_TASKS),
    ],
  },
];

function generateForRepo(config: RepoConfig): GeneratedIssue[] {
  const issues: GeneratedIssue[] = [];
  let globalIndex = 0;

  for (const bucket of config.buckets) {
    const targets = TARGETS[bucket.area as keyof typeof TARGETS] ?? ["module"];

    for (let i = 0; i < bucket.count; i++) {
      globalIndex += 1;
      const template = pick(bucket.templates, i);
      const target = pick(targets, globalIndex);
      const ctx: TaskContext = {
        index: globalIndex,
        area: bucket.area,
        target,
      };

      const labels = [...DEFAULT_LABELS, bucket.label];
      const title = `[${bucket.label}] ${template.title(ctx)}`;

      issues.push({
        repo: config.slug,
        github: config.github,
        number: globalIndex,
        title,
        body: template.body(ctx),
        labels,
        area: bucket.area,
      });
    }
  }

  if (issues.length !== ISSUES_PER_REPO) {
    throw new Error(
      `${config.slug}: expected ${ISSUES_PER_REPO} issues, got ${issues.length}`,
    );
  }

  return issues;
}

function parseArgs(argv: string[]) {
  return {
    publish: argv.includes("--publish"),
    dryRun: argv.includes("--dry-run"),
    help: argv.includes("--help"),
    extra: (() => {
      const i = argv.indexOf("--extra");
      return i >= 0 ? Number(argv[i + 1]) : 0;
    })(),
    repo: (() => {
      const i = argv.indexOf("--repo");
      return i >= 0 ? argv[i + 1] : undefined;
    })(),
    delayMs: (() => {
      const i = argv.indexOf("--delay-ms");
      return i >= 0 ? Number(argv[i + 1]) : 300;
    })(),
  };
}

function sleep(ms: number) {
  return new Promise((r) => setTimeout(r, ms));
}

function fetchExistingTitles(github: string): Set<string> {
  const titles = new Set<string>();
  if (!commandExists("gh")) return titles;

  try {
    const out = execSync(
      `gh issue list --repo ${github} --state all --limit 500 --json title`,
      { encoding: "utf8", stdio: ["pipe", "pipe", "pipe"] },
    );
    const rows = JSON.parse(out) as { title: string }[];
    for (const row of rows) titles.add(row.title);
  } catch {
    console.warn(`Warning: could not list existing issues for ${github}`);
  }
  return titles;
}

function commandExists(cmd: string): boolean {
  return spawnSync("which", [cmd], { stdio: "ignore" }).status === 0;
}

const EXTRA_LABELS = [
  "frontend",
  "contracts",
  "tooling",
  "api",
  "indexer",
  "oracle",
  "workers",
  "shared",
  "sdk",
] as const;

function ensureLabels(github: string) {
  const all = [...DEFAULT_LABELS, TOPUP_LABEL, ...EXTRA_LABELS];
  for (const name of all) {
    spawnSync("gh", ["label", "create", name, "--repo", github, "--force"], {
      stdio: "ignore",
    });
  }
}

function splitCounts(total: number, parts: number): number[] {
  const base = Math.floor(total / parts);
  const rem = total % parts;
  return Array.from({ length: parts }, (_, i) => base + (i < rem ? 1 : 0));
}

function makeUniqueTitle(existing: Set<string>, base: string): string {
  if (!existing.has(base)) return base;
  for (let i = 2; i < 10000; i++) {
    const candidate = `${base} (extra ${i})`;
    if (!existing.has(candidate)) return candidate;
  }
  throw new Error(`Could not make unique title from: ${base}`);
}

function generateTopUpIssues(
  configs: RepoConfig[],
  extraCount: number,
): GeneratedIssue[] {
  if (extraCount <= 0) return [];
  if (!commandExists("gh")) {
    throw new Error(
      "--extra requires gh CLI so we can avoid title collisions (run gh auth login)",
    );
  }

  const counts = splitCounts(extraCount, configs.length);
  const out: GeneratedIssue[] = [];

  for (let c = 0; c < configs.length; c++) {
    const config = configs[c]!;
    const existing = fetchExistingTitles(config.github);
    const perRepo = counts[c]!;

    for (let i = 0; i < perRepo; i++) {
      const bucket = pick(config.buckets, i);
      const template = pick(bucket.templates, i);
      const targets = TARGETS[bucket.area as keyof typeof TARGETS] ?? ["module"];
      const target = pick(targets, i + existing.size);
      const ctx: TaskContext = { index: i + 1, area: bucket.area, target };

      // Keep the same title style as the base generator; ensure uniqueness via suffix.
      const rawTitle = `[${bucket.label}] ${template.title(ctx)}`;
      const title = makeUniqueTitle(existing, rawTitle);
      existing.add(title);

      out.push({
        repo: config.slug,
        github: config.github,
        number: i + 1,
        title,
        body: template.body(ctx),
        labels: [...DEFAULT_LABELS, TOPUP_LABEL, bucket.label],
        area: bucket.area,
      });
    }
  }

  if (out.length !== extraCount) {
    throw new Error(`Expected ${extraCount} top-up issues, got ${out.length}`);
  }
  return out;
}

async function publishIssues(
  issues: GeneratedIssue[],
  delayMs: number,
): Promise<{ created: number; skipped: number }> {
  if (!commandExists("gh")) {
    throw new Error("gh CLI not found — install GitHub CLI and run gh auth login");
  }

  const byRepo = new Map<string, GeneratedIssue[]>();
  for (const issue of issues) {
    const list = byRepo.get(issue.github) ?? [];
    list.push(issue);
    byRepo.set(issue.github, list);
  }

  let created = 0;
  let skipped = 0;

  for (const [github, repoIssues] of byRepo) {
    ensureLabels(github);
    const existing = fetchExistingTitles(github);
    console.log(`Publishing to ${github} (${repoIssues.length} issues)…`);

    for (const issue of repoIssues) {
      if (existing.has(issue.title)) {
        skipped += 1;
        continue;
      }

      const labelArgs = issue.labels.flatMap((l) => ["--label", l]);
      const result = spawnSync(
        "gh",
        [
          "issue",
          "create",
          "--repo",
          github,
          "--title",
          issue.title,
          "--body",
          issue.body,
          ...labelArgs,
        ],
        { encoding: "utf8" },
      );

      if (result.status !== 0) {
        console.error(`Failed: ${issue.title}\n${result.stderr}`);
        continue;
      }

      created += 1;
      existing.add(issue.title);
      if (created % 10 === 0) console.log(`  created ${created}…`);
      await sleep(delayMs);
    }
  }

  return { created, skipped };
}

function writeOutputs(all: GeneratedIssue[], configs: RepoConfig[]) {
  mkdirSync(OUT_DIR, { recursive: true });

  for (const config of configs) {
    const repoIssues = all.filter((i) => i.repo === config.slug);
    const path = join(OUT_DIR, `${config.slug}.json`);
    writeFileSync(path, JSON.stringify(repoIssues, null, 2) + "\n");
    console.log(`Wrote ${path} (${repoIssues.length} issues)`);
  }

  const manifest = {
    generatedAt: new Date().toISOString(),
    total: all.length,
    perRepo: configs.map((c) => ({
      slug: c.slug,
      github: c.github,
      count: all.filter((i) => i.repo === c.slug).length,
    })),
  };
  writeFileSync(
    join(OUT_DIR, "manifest.json"),
    JSON.stringify(manifest, null, 2) + "\n",
  );
}

function printHelp() {
  console.log(`Usage: tsx scripts/generate-issues.ts [options]

Options:
  --publish       Create issues on GitHub (requires gh auth)
  --extra N       Create N additional unique "topup" issues
  --repo <slug>   Only vatix-contract | vatix-backend | swyft
  --delay-ms N    Delay between gh calls (default 300)
  --dry-run       Print 3 sample issues per repo, no files
  --help          Show this help
`);
}

async function main() {
  const args = parseArgs(process.argv.slice(2));
  if (args.help) {
    printHelp();
    return;
  }

  let configs = REPOS;
  if (args.repo) {
    configs = REPOS.filter((r) => r.slug === args.repo);
    if (configs.length === 0) {
      throw new Error(`Unknown repo: ${args.repo}`);
    }
  }

  const base = configs.flatMap(generateForRepo);
  const topUp = generateTopUpIssues(configs, args.extra);
  const all = args.extra > 0 ? topUp : base;

  if (args.dryRun) {
    for (const config of configs) {
      const sample = all.filter((i) => i.repo === config.slug).slice(0, 3);
      console.log(`\n=== ${config.slug} (sample) ===`);
      for (const s of sample) console.log(`- ${s.title}`);
    }
    return;
  }

  writeOutputs(all, configs);

  if (args.publish) {
    const { created, skipped } = await publishIssues(all, args.delayMs);
    console.log(`\nPublish done: ${created} created, ${skipped} skipped (duplicate titles)`);
  } else {
    console.log(
      `\n${all.length} issues written. Run with --publish to create on GitHub.`,
    );
  }
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
