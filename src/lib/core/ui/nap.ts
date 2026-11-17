/** The dog naps after lunch (2–4 PM) and through the night (9 PM–7 AM), local time. */
export function isNapTime(now: Date = new Date()): boolean {
	const hour = now.getHours()
	return (hour >= 14 && hour < 16) || hour >= 21 || hour < 7
}
