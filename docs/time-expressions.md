# Time expressions

Relative: `in 40m`, `40m`, `1h30m`, `2 days 3 hours`. The leading `in` is optional.

Absolute: `at 15:00`, `at 3pm`, `at 3:30 pm`, `at 9`, `today at 18:00`, `tomorrow`,
`tomorrow at 9:30`. A clock time that has already passed means tomorrow, and a bare `tomorrow`
means 09:00.

By day name: `friday`, `fri`, `monday 14`, `friday at 3pm`, `next tuesday`. A day name means the
next one to come round, which is today only when the time has not yet passed, and a bare one
means 09:00. Writing `next` in front rules today out, so on a Wednesday `next friday` is the same
Friday as `friday`.
