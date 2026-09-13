export const plannerMessages = {
  "action.clear": "Clear selection",
  "action.next": "Next page",
  "action.previous": "Previous page",
  "action.retry": "Try again",
  "entity.search": "Search {label}",
  "entity.idle": "Search for an existing item.",
  "entity.loading": "Loading matching items…",
  "entity.stale": "Waiting for results for the current search…",
  "entity.empty": "No matching items.",
  "entity.more": "More matches are available on the next page.",
  "entity.pageLimit": "This result page exceeds the 200-item limit. Refine the search.",
  "entity.identity": "{kind} · {id}",
  "entity.unnamed": "{kind} {id}",
  "entity.selected": "Selected {label}",
  "entity.noneSelected": "No items selected.",
  "entity.remove": "Remove {label}, {kind} {id}",
  "entity.selectionPage": "Showing selected items {start}–{end} of {total}.",
  "entity.readOnly": "This selection is read-only.",
  "duration.unit": "Duration unit",
  "duration.minutes": "Minutes",
  "duration.hours": "Hours",
  "duration.grammar":
    "Use digits and a decimal point, without grouping separators; for example, 1.5 hours.",
  "duration.syntax": "Enter a decimal number using digits and a decimal point.",
  "duration.wholeMinutes": "The duration must be an exact number of whole minutes.",
  "duration.range": "Enter a duration from {minimum} to {maximum} whole minutes.",
  "duration.exactUnit":
    "This duration cannot be written exactly in decimal hours. Keep minutes to preserve its value.",
  "duration.quantity": "{minutes} whole minutes",
  "temporal.zone": "Scenario time zone: {zone}",
  "temporal.startTime": "Start time",
  "temporal.endTime": "End time",
  "temporal.startsAt": "Start local date and time",
  "temporal.endsAt": "End local date and time",
  "temporal.endDayOffset": "End day offset",
  "temporal.sameDay": "Same day",
  "temporal.nextDay": "Next day",
  "temporal.offsetHelp":
    "0 means the same day; 1 means the next day. Values through 255 are supported.",
  "temporal.offsetError": "Enter a whole end day offset from 0 to 255.",
  "temporal.endpoints": "Enter both endpoints. Local times still require native validation.",
  "temporal.timeHelp":
    "Use local time text, such as 09:30 or 09:30:00.123456789. Precision is preserved.",
  "temporal.intervalHelp":
    "Use local date-time text, such as 2026-09-13T09:30. The scenario, not the browser, determines the time zone.",
  "temporal.nativePending":
    "Local time text must be validated and resolved by the application before it is accepted.",
  "strength.required": "Required",
  "strength.preference": "Preference",
  "strength.requiredHelp": "An accepted plan must satisfy every active required rule.",
  "strength.preferenceHelp":
    "Preferences guide optimization without making an otherwise valid plan invalid.",
  "strength.unavailable": "This rule kind is not available in the current implementation.",
  "strength.classLocked": "An existing rule cannot be converted to a different rule class here.",
  "strength.priority": "Preference priority",
  "strength.choosePriority": "Choose a priority",
  "strength.low": "Low",
  "strength.normal": "Normal",
  "strength.high": "High",
  "strength.veryHigh": "Very high",
} as const;

export type PlannerMessageKey = keyof typeof plannerMessages;

export function plannerMessage(
  key: PlannerMessageKey,
  parameters: Readonly<Record<string, string | number>> = {},
  locale?: string,
): string {
  return plannerMessages[key].replace(/\{([A-Za-z][A-Za-z0-9]*)\}/g, (_, name: string) => {
    const value = parameters[name];
    if (value === undefined) return `{${name}}`;
    return typeof value === "number" ? new Intl.NumberFormat(locale).format(value) : value;
  });
}
