# The apps

Each app does one job for one kind of user. Each one is deployed,
updated, and rolled back on its own. A deployment's start page lists
the apps it offers.

## Ride now

**For riders, on their phones.**

Ride now helps a rider take a trip today. It finds stops and
addresses, shows the next departures with live times, and plans trips
on a map. It shows where each bus is, and keeps the rider's saved
stops. It installs like an app and works offline.

**Server:** not needed. With a server it also shows staff-entered
detours and cancellations, searches addresses, and picks up schedule
changes published ahead of time.

## Stop display

**For a screen at a stop or transit center.**

The stop display shows the next departures at one stop, grouped by
route and destination. It shows live times, alerts, and detours. It
runs for weeks with no one watching it, and keeps working if the
network drops.

**Server:** not needed. With a server it also shows staff-entered
detours and cancellations, and picks up schedule changes published
ahead of time.

## Service design

**For service planners.**

Service design helps plan a service change. It shows how the network
performs today, who it reaches, and what a proposed change costs. When
the change is ready, it publishes it for a future date.

**Server:** needed.

## Service change

**For service planners.**

Service change is a second way to plan a service change, built to
compare with Service design. Its plans sit on a timeline: the
schedule running today, the ones before it, and the ones coming.
A published schedule never changes in place; you copy it into a
draft and change the draft. It follows the steps of the work in
order: set up the plan and its calendar (which service runs on which
days, holidays included), scan how often each route runs hour by
hour, look up any stop (where it is, what it is, and every route
that stops there), add a new route (its stops picked in order,
including new stops from the Stops app, and its trips by how often
they run), change one route's day, check the buses and hours the change
needs (each bus's day drawn as a bar, with its trips one click away),
and review the whole proposal for weekdays, Saturdays and
Sundays, against a plan you choose there.

**Server:** needed.

## Stops

**For staff who look after the stops themselves.**

Stops keeps each stop's own story, apart from the schedules: when it
was planned, permitted, built, put in service, suspended for a while,
or retired. Every job gets the whole screen.

- **Stops** shows every stop on a chosen day, coloured by where it is
  in that story, on a map beside a list or as a table. On a phone at
  the stop, "Nearby stops" lists the stops closest to you first.
- **Work** is a table of the work asked for at every stop, by status
  and kind of job.
- **Feed review** shows how the agency's latest schedule feed differs
  from the stops record, with a link to each stop.

A stop opens as a page: where it is, what is there, which routes stop
there that day, the work asked for there, its last site survey, and
everything that has happened to it. From there:

- **Record change**: a new stage, a suspension and the return, a move,
  or new details. It offers only the changes the stop can take that
  day, and says in one line what will be saved.
- **Survey**: the paper stop worksheet as one page, with a list of its
  sections beside it showing how far each is filled. Each answer starts
  from the record, so you only change what has changed, and the side
  column lists the differences before you save. A survey you have not
  finished stays on the device until you save it. On a phone it fills
  the screen.
- **Request work**: the paper installation sheet as a page. A request
  shows where it stands (draft, submitted, work order open, completed)
  and the next step: submit it, open its work order, complete it, then
  update the stop's record.
- **Report issue**: a description and photos, saved as a draft request
  for the planning team to finish.

Place a new stop, or move one, by clicking the map, or type its
latitude and longitude as measured at the stop. A typed point is saved
exactly as typed. "Use my location" gives a starting point. Add photos
from the phone's camera, or a site drawing, to a survey or a request;
only people who are signed in can see them. Sign in to save a change;
looking needs no sign-in. The survey records you as the surveyor, from
your sign-in. A change entered wrong is corrected, not added to.
Schedules read these stops but never change them.

**Server:** needed.

## Read next

[How it works](how-it-works.md) follows one stop display from
start-up to the departures on its screen.
