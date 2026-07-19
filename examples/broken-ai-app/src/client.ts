const { DATABASE_URL, DEEPSEEK_API_KEY } = process.env;

export function config() {
  return {
    databaseUrl: DATABASE_URL,
    deepseekApiKey: DEEPSEEK_API_KEY,
  };
}
