# Handling scrolling and wrapping

## The issue

 - We have a logs DB of 100,000 lines, each of a variable length
 - The user has line wrap enabled
 - The user goes to scoll position X (say, 500,000)

What log does the user see on the top of the view? Nieve answer woulnd be line 500,000, but if line wrap is turned on, then one *log* can take up multiple *lines*. So,depending on how wide the screen is, the user would see some log inbetween 1 and 500,000. 

## Solutions?

Trival way to check this would be to load log n (starting at 1), record the length of the log, and keep enumerating untill we reach the desired scroll position, taking screen wrapping into account. This is not desireable for permormance reasons.

### Attempt #1

Classic way to solve this would be a binary search type thing. For each line we could record the aggregate length alongside it in the DB, do sme cheeky maths

**l**ines = **t**otal_chars / **w**idth

    given w = 10, t = 200; l = 20
    if I scroll to pos 10,
    10 = t / 10
    t = 100

we want to first display character 100.

### Issue

I don't actully know how to get "the closest line to 100" from the DB. And once we get the line, where in the line to start displaying it (it may be wrapped). Also Keeping in mind we also want to support filtering, so the aggregate numbers would be wring if any filtering has been done

### Attempt #2

Very similar to #1, but for each line just store the line length. This solves our filtering issue. Query only the line length and line id, and enumerate though all rows until you reach a point where row+1 should be visible. 

Do the same thing for all further rows until you can fill the screen, and recored the row id's to a proper query for those row ID's only.

psudo query:

    // don't select message itself. Can also inclide filter.
    select range 0..500_000, line_len, line_id