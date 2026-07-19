# Broken AI App Example

Run this from the example directory:

```bash
driftguard check
```

Expected failures:

- `DEEPSEEK_API_KEY` is used in `src/client.ts` but missing from `.env.example`.
- `src/prompts/router.md` requires `{{customer_tier}}`, but the golden fixture does not provide `input.customer_tier`.
- The golden fixture output uses `route`, but the schema requires `destination`.

