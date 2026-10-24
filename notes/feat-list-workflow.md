Last verified at commit `be8e74e4dc492e761dc082a8805c91695ae380b6`.

You're bringing the Linux application up to feature parity with the macOS application.

The list of features to implement is in [feat-list](feat-list.md). Use the "Suggested order" section at the bottom of the section as a guide to the implementation order.

- Start a new subagent, the orchestrator. This is what the orchestrator will do:
    - Create a new branch, `linux/test-fixes`
    - Use the /orchestrate skill to verify that all of the Linux tests and linters pass, and fix any failures
    - Merge any changes back into the `linux` branch

Now we start the main loop:

- Use [feat-list](feat-list.md) to find a feature which has not yet been implemented
- Start a new subagent, the orchestrator. This is what the orchestrator will do:
    - Create a new branch, `linux/feat/<feature-name>`
    - Add the feature using the `/orchestrate` skill
    - Once tests pass, linters pass, the application builds, etc., marks it done in the feature list
    - Merges the work back into the `linux` branch
- Move on to the next one
- If you have questions that only the user can answer (feature or architecture decisions), you or a subagent can stop the loop until I respond

The goal for you is to keep your context relatively clear: you should know what the contents of the feature list are, what task is being worked on, whether it's finished, etc.
