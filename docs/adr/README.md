# Architecture Decision Records (ADRs)

This directory contains Architecture Decision Records (ADRs) for the Vatix Protocol smart contract system.

## What is an ADR?

An Architecture Decision Record (ADR) is a document that captures an important architectural decision made along with its context and consequences. ADRs help teams:

- **Document rationale**: Explain why decisions were made, not just what was decided
- **Preserve context**: Capture the problem, constraints, and alternatives considered
- **Enable review**: Allow future team members to understand and question past decisions
- **Track evolution**: Show how the architecture has evolved over time

## When to Write an ADR

Write an ADR when you make a decision that:

- Has significant impact on the system architecture
- Affects multiple components or modules
- Involves tradeoffs between competing concerns
- Will be difficult or expensive to change later
- Needs to be understood by future maintainers

Examples:
- Choosing between oracle integration approaches
- Selecting a data structure or storage pattern
- Deciding on a security model or access control mechanism
- Adopting a new dependency or external service

## ADR Format

Each ADR should follow this structure:

1. **Title**: Descriptive name of the decision
2. **Status**: Proposed, Accepted, Deprecated, or Superseded
3. **Context**: Problem statement, requirements, constraints
4. **Decision**: What was decided and the approach taken
5. **Consequences**: Positive, negative, and neutral outcomes
6. **Alternatives Considered**: Other options and why they were rejected

## ADR Lifecycle

- **Proposed**: Decision is under discussion
- **Accepted**: Decision has been approved and is being implemented
- **Deprecated**: Decision is no longer recommended but not yet replaced
- **Superseded**: Decision has been replaced by a newer ADR (reference the new one)

## ADR List

| Number | Title | Status | Date |
|--------|-------|--------|------|
| [001](./001-oracle-adapter-selection.md) | Oracle Adapter Selection for Market Resolution | Accepted | 2026-06-29 |

## Contributing

When creating a new ADR:

1. Copy the template from an existing ADR
2. Number it sequentially (next available number)
3. Write in present tense (as if the decision is happening now)
4. Be concise but thorough
5. Include code examples where helpful
6. Reference related issues, PRs, and documentation
7. Update this README's ADR list

## Further Reading

- [Michael Nygard's ADR article](https://cognitect.com/blog/2011/11/15/documenting-architecture-decisions)
- [ADR GitHub organization](https://adr.github.io/)
- [Spotify's ADR process](https://engineering.atspotify.com/2020/04/14/when-should-i-write-an-architecture-decision-record/)

---

**Last Updated:** 2026-06-29
