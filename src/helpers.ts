export function formatTime(value: string, en: boolean): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return '';
  return new Intl.DateTimeFormat(en ? 'en-US' : 'ru-RU', {
    hour: '2-digit', minute: '2-digit',
    ...(new Date().toDateString() !== date.toDateString() ? { day: 'numeric', month: 'short' } : {}),
  }).format(date);
}
