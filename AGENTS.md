---
description: Apply these instructions to ensure all code generation and reviews align with strict, language-agnostic production standards.
applyTo: '**'
---

# Operational Protocol
Before executing any task, you must strictly follow this sequence:

**Step 1: Acknowledge.** Explicitly confirm your understanding of and adherence to these universal rules.
**Step 2: Audit.** Identify and list all files, modules, or components required for the task.
**Step 3: Blueprint.** Outline a concise, high-level architectural plan of action before writing any code.
**Step 4: Execution.** Deliver complete, production-ready code. You are strictly forbidden from using snippets, placeholders (e.g., `TODO`, `pass`, `...`), or stubs. 
**Step 5: Autonomy.** If context, imports, or dependencies are missing, autonomously resolve them by defaulting to the language's standard library or canonical implementation practices.

# Coding Standards

## 1. General Requirements
* **Self-Documenting Logic:** Do not rely on inline comments to explain behavior. Variables, functions, and architecture must clearly dictate intent.
* **Hermetic Code:** Every file must be entirely self-contained. Include all necessary imports, headers, and dependencies. The code must compile or execute immediately as-is.
* **Total Implementation:** Every function, class, and method must contain final, working logic. Mocks and no-ops are strictly prohibited unless explicitly designing a testing suite.

## 2. Resource & State Management
* **Lifecycle Strictness:** Explicitly manage memory, connections, and file handles using the most robust native paradigm available (e.g., RAII, context managers, garbage collection optimization, or strict ownership rules).
* **Immutability by Default:** Enforce state immutability wherever possible using language-native constraints (e.g., `const`, `readonly`, `final`). Limit mutable state to strictly scoped, necessary components.

## 3. Error Handling & Safety
* **Explicit Propagation:** Handle all edge cases and errors natively. Use idiomatic error flow (e.g., Result/Option types, strictly caught exceptions, or multiple return values).
* **Zero Panics/Crashes:** Never use forceful unwraps, assertions that crash in production, or unhandled panic equivalents. Failures must be gracefully handled or propagated contextually.

## 4. Verification & Quality
* **Strict Typing:** Utilize strict/static typing tools native to the language or ecosystem. Avoid dynamic or "any" types unless categorically required by the architecture.
* **Zero Warnings:** Code must be formatted and structured to pass the target language’s strictest standard linter and compiler settings without a single warning or error.