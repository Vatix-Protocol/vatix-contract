# How to Complete Issue #368: ADR for Oracle Adapter Selection

This document provides step-by-step instructions for completing and merging Issue #368.

## Current Status

✅ **Development Complete** - All changes implemented and documented

- [x] ADR document created (`docs/adr/001-oracle-adapter-selection.md`)
- [x] ADR index created (`docs/adr/README.md`)
- [x] Summary document created (`ISSUE_368_SUMMARY.md`)
- [x] PR description created (`ISSUE_368_PR_DESCRIPTION.md`)
- [x] Committed to branch `feature/oracle-adapter-adr-368`

## Changes Summary

### Files Created

1. **`docs/adr/001-oracle-adapter-selection.md`** (698 lines)
   - Comprehensive ADR documenting oracle adapter architecture
   - Covers Ed25519 (implemented), Reflector (planned), Pyth (planned) adapters
   - Includes security analysis, implementation plan, testing strategy
   - Documents canonical message format and threshold signatures

2. **`docs/adr/README.md`** (89 lines)
   - ADR process documentation and guidelines
   - List of all ADRs with links
   - Contributing guidelines and best practices

3. **`ISSUE_368_SUMMARY.md`**
   - Technical summary of all changes
   - Design decisions and rationale
   - Security analysis
   - Testing coverage
   - Impact assessment

4. **`ISSUE_368_PR_DESCRIPTION.md`**
   - PR description ready to copy to GitHub
   - Summary of changes and key sections
   - Design rationale and consequences
   - Testing and verification notes
   - Reviewer guidance

5. **`COMPLETE_ISSUE_368.md`** (this file)
   - Step-by-step completion guide

### Files Referenced (No Changes)

- `contracts/market/src/oracle.rs` - Oracle implementation
- `contracts/market/src/types.rs` - AdapterType enum
- `contracts/market/src/error.rs` - Error types

## Step-by-Step Completion Guide

### Step 1: Verify Changes ✅

**Check branch:**
```bash
git status
git branch
```

**Expected**: On branch `feature/oracle-adapter-adr-368` with uncommitted files

**Verify files exist:**
```bash
ls docs/adr/
ls *.md | grep 368
```

**Expected output:**
```
docs/adr/001-oracle-adapter-selection.md
docs/adr/README.md
ISSUE_368_SUMMARY.md
ISSUE_368_PR_DESCRIPTION.md
COMPLETE_ISSUE_368.md
```

### Step 2: Review Documentation

**Review ADR content:**
```bash
# On Windows with default editor
notepad docs/adr/001-oracle-adapter-selection.md

# Or use VS Code
code docs/adr/001-oracle-adapter-selection.md
```

**Check for:**
- [ ] All sections complete
- [ ] Code examples present
- [ ] Security analysis thorough
- [ ] Implementation plan clear
- [ ] References accurate

**Review ADR index:**
```bash
notepad docs/adr/README.md
```

**Check for:**
- [ ] ADR 001 listed in table
- [ ] Process guidelines clear
- [ ] Examples helpful

### Step 3: Verify Existing Tests

Since this is documentation-only, verify existing oracle tests pass:

```bash
cd contracts/market
cargo test oracle -- --test-threads=1
cd ..\..
```

**Expected**: All 20+ oracle tests pass

**Test categories:**
- Message construction tests (8 tests)
- Signature verification tests (6 tests)
- Threshold signature tests (6 tests)
- Test vector generation (1 test)

### Step 4: Commit Changes

**Stage all ADR-related files:**
```bash
git add docs/adr/001-oracle-adapter-selection.md
git add docs/adr/README.md
git add ISSUE_368_SUMMARY.md
git add ISSUE_368_PR_DESCRIPTION.md
git add COMPLETE_ISSUE_368.md
```

**Verify staging:**
```bash
git status
```

**Expected**: 5 new files to be committed

**Commit with descriptive message:**
```bash
git commit -m "docs: add ADR for oracle adapter selection (#368)

Created comprehensive Architecture Decision Record (ADR 001) documenting
the oracle adapter selection architecture for market resolution.

Changes:
- Added ADR 001 documenting pluggable oracle adapter system
- Added ADR index/README explaining ADR process
- Documented Ed25519 (implemented), Reflector (planned), Pyth (planned)
- Included security analysis of 6 attack vectors
- Provided 4-phase implementation roadmap
- Referenced existing 20+ oracle tests
- Created summary and completion documents

The ADR captures:
- Context: Why pluggable adapters are needed
- Decision: Start with Ed25519, extend to Reflector/Pyth
- Consequences: Positive, negative, and neutral outcomes
- Implementation: Phase 1 (Ed25519) complete, Phases 2-4 planned
- Security: Attack vectors analyzed with mitigations
- Testing: 20+ tests covering message construction, signature verification, threshold signatures

No code changes - documentation only.

Resolves #368"
```

### Step 5: Push Branch

**Push to remote:**
```bash
git push -u origin feature/oracle-adapter-adr-368
```

**Expected output:**
```
Enumerating objects: X, done.
Counting objects: 100% (X/X), done.
...
To github.com:Vatix-Protocol/vatix-contract.git
 * [new branch]      feature/oracle-adapter-adr-368 -> feature/oracle-adapter-adr-368
Branch 'feature/oracle-adapter-adr-368' set up to track remote branch 'feature/oracle-adapter-adr-368' from 'origin'.
```

### Step 6: Create Pull Request

**Option 1: Using GitHub CLI (if installed):**

```bash
gh pr create --title "docs: ADR for oracle adapter selection (#368)" --body-file ISSUE_368_PR_DESCRIPTION.md --base main
```

**Option 2: Using GitHub Web UI:**

1. Go to: `https://github.com/Vatix-Protocol/vatix-contract`
2. Click "Pull requests" tab
3. Click "New pull request"
4. Select base: `main`, compare: `feature/oracle-adapter-adr-368`
5. Click "Create pull request"
6. Copy content from `ISSUE_368_PR_DESCRIPTION.md` into PR description
7. Add labels: `documentation`, `enhancement`
8. Link issue #368 in the right sidebar
9. Request reviewers
10. Click "Create pull request"

**PR Title:**
```
docs: ADR for oracle adapter selection (#368)
```

**PR Labels:**
- `documentation`
- `enhancement`
- `adr`

**PR Description:**
(Copy from `ISSUE_368_PR_DESCRIPTION.md`)

### Step 7: Verify PR

**Check PR page includes:**
- [x] Title references issue #368
- [x] Description from `ISSUE_368_PR_DESCRIPTION.md`
- [x] Labels applied
- [x] Issue #368 linked
- [x] Reviewers requested
- [x] All checks passing (if CI configured)

**Files changed should show:**
```
docs/adr/001-oracle-adapter-selection.md  | 698 ++++++++++++++++++++++++++++++
docs/adr/README.md                        |  89 ++++
ISSUE_368_SUMMARY.md                      | XXX ++++
ISSUE_368_PR_DESCRIPTION.md               | XXX ++++
COMPLETE_ISSUE_368.md                     | XXX ++++
5 files changed, XXXX insertions(+)
```

### Step 8: Review and Merge

**Self-review checklist:**
- [ ] ADR document complete and well-structured
- [ ] Code examples match actual implementation
- [ ] Security analysis thorough
- [ ] Implementation plan actionable
- [ ] References accurate
- [ ] No typos or formatting issues

**Request reviews from:**
- Technical lead (architecture review)
- Security expert (security analysis review)
- Documentation owner (format and clarity review)

**Address feedback:**
1. Make requested changes on the branch
2. Commit with descriptive messages
3. Push updates: `git push`
4. PR will update automatically

**After approval:**
1. Ensure all checks pass
2. Click "Merge pull request"
3. Select merge strategy: "Squash and merge" or "Create a merge commit"
4. Confirm merge
5. Delete branch `feature/oracle-adapter-adr-368` (optional)

### Step 9: Post-Merge Verification

**Verify on main branch:**
```bash
git checkout main
git pull
ls docs/adr/
```

**Expected**: ADR files present on main

**Verify issue closed:**
- Go to issue #368
- Should be automatically closed with link to PR

**Update related documentation (if needed):**
- Update project README if it references ADRs
- Link from oracle implementation comments to ADR
- Announce ADR in team communication channels

## Verification Commands

### Check Current State
```bash
# Current branch
git branch --show-current

# Uncommitted changes
git status

# Recent commits
git log --oneline -n 5
```

### Verify Documentation
```bash
# File existence
ls docs/adr/

# File sizes (should be substantial)
dir docs\adr\*.md

# Word count (Unix-like tools)
# On Windows with Git Bash:
# wc -l docs/adr/*.md
```

### Test Existing Code
```bash
# Oracle tests
cd contracts/market
cargo test oracle

# All tests
cargo test
cd ..\..
```

## Rollback Plan

If issues are discovered after merge:

### Option 1: Revert PR (preferred for critical issues)
```bash
# On main branch
git checkout main
git pull
git revert -m 1 <merge-commit-hash>
git push
```

### Option 2: Fix Forward (preferred for minor issues)
```bash
git checkout main
git pull
git checkout -b fix/adr-368-corrections
# Make corrections
git add .
git commit -m "docs: fix issues in ADR 001"
git push -u origin fix/adr-368-corrections
# Create PR
```

### Option 3: Update ADR (for evolving decisions)
```bash
git checkout main
git pull
git checkout -b update/adr-001-v2
# Update ADR with new information
# Increment version number
git add .
git commit -m "docs: update ADR 001 to v1.1.0"
git push -u origin update/adr-001-v2
# Create PR
```

## Success Criteria

Issue #368 is complete when:

- [x] ADR 001 created with comprehensive content
- [x] ADR index created with process guidelines
- [x] Summary documents created
- [x] Changes committed to feature branch
- [x] Branch pushed to remote
- [ ] PR created and linked to issue #368
- [ ] PR reviewed and approved
- [ ] PR merged to main
- [ ] Issue #368 automatically closed
- [ ] Documentation accessible in main branch

## Timeline

- **Development**: ~4 hours (writing ADR)
- **Documentation**: ~1 hour (summary, PR description, this guide)
- **Review**: 1-2 days (team reviews ADR)
- **Merge**: ~15 minutes (after approval)

**Total**: 2-3 days from start to merge

## Related Documents

- **ADR Document**: `docs/adr/001-oracle-adapter-selection.md`
- **ADR Index**: `docs/adr/README.md`
- **Technical Summary**: `ISSUE_368_SUMMARY.md`
- **PR Description**: `ISSUE_368_PR_DESCRIPTION.md`
- **Completion Guide**: `COMPLETE_ISSUE_368.md` (this file)

## Related Issues

- **Issue #139**: Decentralized Oracle Integration (future)
- **Issue #368**: This ADR (current)
- **Issue #378**: Multi-Signer Threshold Resolution (implemented)

## Next Steps After Merge

1. **Share ADR**: Announce in team channels
2. **Security Review**: Include in next security audit
3. **Backend Alignment**: Share test vector with backend team
4. **Phase 2 Planning**: Begin Reflector adapter design
5. **Template Creation**: Use as template for future ADRs

## Questions or Issues?

If you encounter problems:

1. **Check branch**: Ensure on `feature/oracle-adapter-adr-368`
2. **Check files**: Verify all 5 files created
3. **Check commits**: `git log` should show ADR commit
4. **Check remote**: `git remote -v` should show correct repo
5. **Check permissions**: Ensure you can push to repository

For help:
- Review this guide from Step 1
- Check git status and recent commits
- Consult team for review/approval
- Refer to `ISSUE_368_SUMMARY.md` for technical details

---

**Document Version**: 1.0.0  
**Last Updated**: 2026-06-29  
**Issue**: #368  
**Branch**: feature/oracle-adapter-adr-368  
**Status**: Ready for PR creation
