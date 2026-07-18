export function deepseekHeaders() {
  return {
    Authorization: `Bearer ${process.env.DEEPSEEK_API_KEY}`,
  };
}
