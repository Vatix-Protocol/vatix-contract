// Unit tests for `--publish` duplicate-skip reporting and exit behavior (#1052).
//
// Run with: `node --test --import tsx scripts/generate-issues-publish.test.ts`
import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import {
  formatPublishSummary,
  handlePublishExit,
  publishIssues,
  writePublishSummaryToManifest,
  type GeneratedIssue,
} from "./generate-issues";

function makeIssue(number: number, title: string): GeneratedIssue {
  return {
    repo: "vatix-contract",
    github: "Vatix-Protocol/vatix-contract",
    number,
    title,
    body: "## Context\nTest\n\n## Task\nTest\n\n## Acceptance criteria\n- [ ] Test\n\n## Notes\n- Test",
    labels: ["good first issue", "easy", "frontend"],
    area: "web",
  };
}

test("publishIssues skips existing duplicate titles and exits 0 when all issues already exist", async () => {
  const issues = [
    makeIssue(1, "[frontend] Add loading skeleton to Navbar"),
    makeIssue(2, "[frontend] Add empty state copy for MarketCard"),
  ];

  let createCalls = 0;
  const result = await publishIssues(issues, 0, {
    checkGh: () => true,
    ensureLabelsFn: () => {},
    fetchExistingTitlesFn: () => new Set(issues.map((i) => i.title)),
    createIssueFn: () => {
      createCalls += 1;
      return { status: 0, stderr: "" };
    },
  });

  assert.equal(createCalls, 0);
  assert.deepEqual(result, {
    created: 0,
    skipped: 2,
    failed: [],
  });
  assert.equal(
    formatPublishSummary(result),
    "created=0 skipped_duplicates=2 failed=0",
  );

  let exitedWith: number | undefined;
  handlePublishExit(result, (code) => {
    exitedWith = code;
  });
  assert.equal(exitedWith, undefined, "must exit 0 when failed.length === 0");
});

test("publishIssues records failing issue titles and handlePublishExit exits non-zero when failed.length > 0", async () => {
  const issues = [
    makeIssue(1, "[frontend] Add loading skeleton to Navbar"),
    makeIssue(2, "[frontend] Add empty state copy for MarketCard"),
    makeIssue(3, "[contracts] Add unit test case for deposit module"),
  ];

  const result = await publishIssues(issues, 0, {
    checkGh: () => true,
    ensureLabelsFn: () => {},
    fetchExistingTitlesFn: () =>
      new Set(["[frontend] Add loading skeleton to Navbar"]),
    createIssueFn: (_github, issue) => {
      if (issue.number === 3) {
        return { status: 1, stderr: "HTTP 502 Bad Gateway" };
      }
      return { status: 0, stderr: "" };
    },
  });

  assert.deepEqual(result, {
    created: 1,
    skipped: 1,
    failed: ["[contracts] Add unit test case for deposit module"],
  });
  assert.equal(
    formatPublishSummary(result),
    "created=1 skipped_duplicates=1 failed=1",
  );

  let exitedWith: number | undefined;
  handlePublishExit(result, (code) => {
    exitedWith = code;
  });
  assert.equal(exitedWith, 1, "must exit non-zero when failed.length > 0");
});

test("writePublishSummaryToManifest persists publishSummary counts into manifest.json", () => {
  const dir = mkdtempSync(join(tmpdir(), "vatix-manifest-"));
  try {
    const manifestPath = join(dir, "manifest.json");
    writeFileSync(
      manifestPath,
      JSON.stringify({ generatedAt: "2026-01-01T00:00:00.000Z", total: 125 }),
    );

    writePublishSummaryToManifest(
      {
        created: 5,
        skipped: 119,
        failed: ["[tooling] Document usage of deploy.sh"],
      },
      dir,
    );

    const parsed = JSON.parse(readFileSync(manifestPath, "utf8"));
    assert.equal(parsed.total, 125);
    assert.deepEqual(parsed.publishSummary, {
      created: 5,
      skippedDuplicates: 119,
      failed: 1,
      failedTitles: ["[tooling] Document usage of deploy.sh"],
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
