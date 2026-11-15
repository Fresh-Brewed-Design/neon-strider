import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri/target", ".claude"] },
  {
    extends: [js.configs.recommended, ...tseslint.configs.recommended],
    files: ["**/*.{ts,tsx}"],
    languageOptions: {
      ecmaVersion: 2020,
      globals: globals.browser,
    },
    plugins: {
      "react-hooks": reactHooks,
      "react-refresh": reactRefresh,
    },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "react-refresh/only-export-components": ["warn", { allowConstantExport: true }],
      "@typescript-eslint/no-explicit-any": "error",
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
      // Native dialogs are unthemed OS boxes: confirm through the store's `confirm`
      // (ConfirmDialog) and show results under the control with <ActionStatus>.
      "no-alert": "error",
      "no-restricted-globals": [
        "error",
        ...["alert", "confirm", "prompt"].map((name) => ({
          name,
          message: "Use the store's `confirm` (ConfirmDialog) or <ActionStatus>, not a native dialog.",
        })),
      ],
    },
  },
);
