import { describe, expect, it, vi } from "vitest";

// The generated Soroban bindings are build artifacts and are not checked in;
// stub them so this focused authz test can exercise `index.ts` standalone.
vi.mock("./market", () => ({}));
vi.mock("./treasury", () => ({}));
vi.mock("./outcome-token", () => ({}));
vi.mock("./resolution", () => ({}));

import {
  assertAuthorized,
  ContractCallError,
  ContractErrorCode,
  PRIVILEGED_ENTRYPOINTS,
} from "./index";

function expectDenied(
  entrypoint: string,
  role: string | null | undefined,
  correlationId: string,
): void {
  let thrown: unknown;
  try {
    assertAuthorized(entrypoint, role, correlationId);
  } catch (error) {
    thrown = error;
  }

  expect(thrown).toBeInstanceOf(ContractCallError);
  const error = thrown as ContractCallError;
  expect(error.code).toBe(ContractErrorCode.UNAUTHORIZED);
  expect(error.correlationId).toBe(correlationId);
  expect(error.message).toContain(entrypoint);
}

describe("assertAuthorized", () => {
  it("denies every privileged entrypoint for non-admin roles with a typed error", () => {
    expect(PRIVILEGED_ENTRYPOINTS.length).toBeGreaterThan(0);

    for (const entrypoint of PRIVILEGED_ENTRYPOINTS) {
      const correlationId = `cid-${entrypoint}`;
      for (const role of [null, undefined, "user"]) {
        expectDenied(entrypoint, role, correlationId);
      }
      expect(() => assertAuthorized(entrypoint, "admin", correlationId)).not.toThrow();
    }
  });

  it("treats entrypoints outside the privileged list as unprivileged", () => {
    expect(() =>
      assertAuthorized("not_a_privileged_entrypoint", "user", "cid-unlisted"),
    ).not.toThrow();
  });
});
