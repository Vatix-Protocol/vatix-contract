// Unit tests for generated issue payload schema validation (#1057).
//
// Run with: `node --test --import tsx scripts/generate-issues.test.ts`
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  REQUIRED_SECTIONS,
  validateIssue,
  validateIssuesOrExit,
  writeOutputs,
  publishIssues,
  type GeneratedIssue,
} from "./generate-issues";

function makeValidIssue(overrides: Partial<GeneratedIssue> = {}): GeneratedIssue {
  return {
    repo: "vatix-contract",
    github: "Vatix-Protocol/vatix-contract",
    number: 1,
    title: "[frontend] Add loading skeleton to Navbar",
    body: [
      "## Context",
      "Small onboarding task for **web**.",
      "",
      "## Task",
      "Add loading skeleton to Navbar",
      "",
      "## Acceptance criteria",
      "- [ ] Skeleton shows while data is loading",
      "",
      "## Notes",
      "- Keep the PR focused on this task only",
    ].join("\n"),
    labels: ["good first issue", "easy", "frontend"],
    area: "web",
    ...overrides,
  };
}

test("REQUIRED_SECTIONS lists the four canonical markdown headings", () => {
  assert.deepEqual(REQUIRED_SECTIONS, [
    "## Context",
    "## Task",
    "## Acceptance criteria",
    "## Notes",
  ]);
});

test("validateIssue returns no violations for a valid issue payload", () => {
  const issue = makeValidIssue();
  assert.deepEqual(validateIssue(issue), []);
});

test("validateIssue catches a missing '## Acceptance criteria' section", () => {
  const issue = makeValidIssue({
    body: [
      "## Context",
      "Context text",
      "## Task",
      "Task text",
      "## Notes",
      "Notes text",
    ].join("\n"),
  });
  const violations = validateIssue(issue);
  assert.ok(
    violations.some((v) => v.includes("## Acceptance criteria")),
    `expected missing section violation, got: ${violations.join(", ")}`,
  );
});

test("validateIssue catches an unsubstituted {target} placeholder in title or body", () => {
  const inTitle = makeValidIssue({
    title: "[frontend] Add loading skeleton to {target}",
  });
  assert.ok(
    validateIssue(inTitle).some((v) => v.includes("{target}")),
    "expected {target} violation in title",
  );

  const inBody = makeValidIssue({
    body: makeValidIssue().body.replace("Navbar", "{target}"),
  });
  assert.ok(
    validateIssue(inBody).some((v) => v.includes("{target}")),
    "expected {target} violation in body",
  );
});

test("validateIssue catches an empty title and an empty labels array", () => {
  const emptyTitle = makeValidIssue({ title: "   " });
  assert.ok(
    validateIssue(emptyTitle).some((v) => v.includes("title")),
    "expected empty title violation",
  );

  const emptyLabels = makeValidIssue({ labels: [] });
  assert.ok(
    validateIssue(emptyLabels).some((v) => v.includes("labels")),
    "expected empty labels violation",
  );
});

test("validateIssuesOrExit names the offending issue and aborts before writeOutputs or publishIssues", async () => {
  const broken = makeValidIssue({
    number: 42,
    title: "[contracts] Broken template for {target}",
    labels: [],
  });

  const logged: string[] = [];
  let exitCode: number | undefined;

  assert.throws(
    () =>
      validateIssuesOrExit([makeValidIssue(), broken], {
        errorLog: (msg) => logged.push(msg),
        exitFn: (code) => {
          exitCode = code;
          throw new Error(`process.exit(${code})`);
        },
      }),
    /process\.exit\(1\)/,
  );

  assert.equal(exitCode, 1);
  assert.equal(logged.length, 1);
  assert.match(logged[0]!, /vatix-contract#42/);
  assert.match(logged[0]!, /Broken template for \{target\}/);

  // Verify writeOutputs and publishIssues also refuse to proceed on invalid issues
  const origExit = process.exit;
  const origError = console.error;
  try {
    process.exit = ((code?: number) => {
      throw new Error(`exit:${code}`);
    }) as never;
    console.error = () => {};

    assert.throws(() => writeOutputs([broken], []), /exit:1/);
    await assert.rejects(() => publishIssues([broken], 0), /exit:1/);
  } finally {
    process.exit = origExit;
    console.error = origError;
  }
});
