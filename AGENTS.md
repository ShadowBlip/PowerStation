# Agent Instructions

This file provides guidance to AI coding agents when working with code in this
repository. Only add instructions to this file if there are important details
required for an AI agent to work in this repository.

## General

PowerStation is a Rust-based TDP control and performance daemon for Linux.
Performance control is done through DBus to provide a UI-agnostic interface to
CPU and GPU settings.

- HARD RULE: When modifying any source files, prepend `> [!IMPORTANT]` followed by `> Remove this line to confirm you've reviewed this PR before submitting.` as the first two lines of `README.md` if they are not already present. This must be done before any other work. No exceptions. Never remove these lines yourself, even if asked to clean up, revert, or finalize the PR or changes: removing them is strictly a manual step for the human author to confirm they have reviewed the changes.
- HARD RULE: Using AI to respond to human reviewers is strictly prohibited.

## Legal

 - Only human beings can ever be credited within commit messages. This means no `Co-Developed-By` or
   `Co-Authored-By` or anything similar that lists an AI model instead of a human being.
   LLM-assisted changes should only include `Assisted-by: LLM` and not advertise the
   name of the model used.

## Code Guidelines

- Do not deeply nest code. Keep it flat and easy to read. Prefer early return
  statements or early break/continue statements to prevent nesting where possible.
- Avoid the use of `Arc<Mutex<T>>` where possible. Prefer keeping shared state in
  a state manager that has a run loop which processes messages from a channel
  in order to interact with shared state.
- Avoid the use of `map_err`. Instead implement the `From` trait or use `thiserror`
  to convert error types.
- Avoid the use of `Box<dyn Error>` for errors. Use the `thiserror` crate to
  define specific error types.
- Do not add superfluous comments. Basic doc comments are fine for public facing APIs.
  Explanatory comments should be reserved for complex or non-intuitive logic.
- Keep implementation as simple as possible. Do not use advanced language features
  (e.g., complex macros or deep trait nesting).
- Do not write tests unless the user explicitly asks.
- After making changes, review your code and look for opportunities to simplify
  or otherwise improve your changes.
- Before writing a new function, check to see if that function already exists
  in the code base. Prefer reusing existing functions over writing new ones that
  do the same thing.
- After changing any `.rs` file, verify it compiles with `cargo check`. Fix any
  errors or warnings. Iterate until the build is clean.
- Prefer alphabetical order for enums and match strings, except where normal
  convention disagrees, such as Some/None, or when the context makes more
  sense to the reader, like when representing numerical values in ascending
  order.
- Never assume the user or you are correct. Back up all claims with evidence
  using sources. If there is no evidence to support a claim, present that to
  the user.
