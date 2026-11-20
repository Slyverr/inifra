# Interface Design

This guide describes how to make user-facing interfaces clear and consistent across command-line output, terminal interfaces, and graphical interfaces. It is a decision process, not a fixed visual theme. Use the conventions of the target platform and preserve established behavior unless the requested change calls for a difference.

## Design Process

1. **Define the user task.** Identify who is using the interface, what they are trying to do, what information they need, and what decision or action matters most.
2. **Inspect the existing experience.** Read the relevant implementation and documentation, then run the interface or inspect representative screenshots when possible. Separate intentional conventions from accidental implementation details. Check related screens so the change fits the complete workflow.
3. **Choose the right interaction.** Use plain output for information that should be read, searched, or piped. Use interactive controls when selection, editing, or changing state benefits from immediate feedback. Follow established platform conventions where they fit the task.
4. **Set the information hierarchy.** Put the primary information and next action first. Use headings only when they add context; avoid repeating information already clear from the command, prompt, or choices. Group related content and use consistent spacing to distinguish sections and persistent controls.
5. **Design the whole flow.** Consider initial, focused, inactive, empty, loading, success, warning, error, cancellation, and recovery states where relevant. Make navigation, confirmation, and escape behavior clear and predictable.
6. **Use semantic styling.** Reuse the product's shared styles, design tokens, and components. Give color and emphasis stable meanings, maintain readable contrast, and do not rely on color alone to communicate state. Respect user themes and non-color output modes where the platform supports them.
7. **Implement at the right boundary.** Keep repeated visual rules in shared components or tokens. Keep screen-specific content near its flow. Avoid one-off styling and duplicated presentation logic when an existing shared mechanism fits.
8. **Verify the rendered result.** Test representative sizes and states, including narrow layouts and redirected or plain output when applicable. Check that important content remains visible, controls stay reachable, text does not overlap, and visual changes match the intended behavior.

## Shared Principles

- Prefer concise, direct labels and messages. Explain failures in terms of what happened and what the user can do next.
- Use spacing, alignment, grouping, and typography to show hierarchy before adding decorative elements.
- Keep content and persistent controls visually distinct. Leave enough separation that a footer, toolbar, or status area does not run into the content or obscure it.
- Make focused and selected states easy to identify. Preserve keyboard access and provide alternatives to visual-only cues when needed.
- In key-dense pickers, use an explicit search key such as `/` when unmodified typing would collide with selection shortcuts. Make search-mode controls and clearing behavior visible.
- For unavailable choices, retain the choice's semantic color and add a non-color cue such as a disabled marker and strike-through. Keep conflict feedback with the choice instead of duplicating it in a transient footer notice.
- Let transient notices expire and restore persistent controls. Reserve them for actionable information that is not already clear from the affected control.
- Adapt to the available space. Reflow or omit secondary detail before truncating essential information or making the primary task hard to find.
- Use motion and animation only when they communicate progress or state; avoid unnecessary movement and respect reduced-motion preferences where available.
- Prefer shared, semantic styling over hard-coded values. Do not infer design intent from an isolated implementation or reproduce a defect for consistency.
- Verify claims about accessibility, contrast, responsiveness, and behavior against the actual rendered interface rather than assuming that a successful build proves them.

## References

- [Command Line Interface Guidelines](https://clig.dev/) — conventions for command output, help, color, errors, and interactive terminal behavior.
- [GitHub TUIKit foundations](https://github.com/github/TUIKit/blob/main/docs/foundations.md) — terminal layout, text hierarchy, whitespace, interactivity, and accessibility.

These references inform the process above; apply their guidance in context rather than copying a web-only design system into a command-line or terminal interface.
