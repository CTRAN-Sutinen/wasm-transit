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
that stops there), change one route's day, check the buses and hours the change
needs (each bus's day drawn as a bar, with its trips one click away),
and review the whole proposal for weekdays, Saturdays and
Sundays, against a plan you choose there.

**Server:** needed.

## Stops

**For staff who look after the stops themselves.**

Stops keeps each stop's own story, apart from the schedules: when it
was planned, permitted, built, put in service, suspended for a while,
or retired. Pick a day to see every stop as it stood then, coloured by
where it is in that story. Open a stop to see what is there (a
shelter, a bench, lighting), which routes stop there that day, and
everything that has happened to it. Record a change on the day it
happens: a new stop, its next step, a suspension and the return, a
move, or new details. Place a new stop, or move one, by clicking the
map, or type its latitude and longitude as measured at the stop. A
typed point is saved exactly as typed. On a phone at the stop, "Use
my location" gives a starting point. Survey a stop on a phone or
tablet: the questions on the paper stop worksheet, what is at the
stop, and its numbers in other systems. A survey you have not
finished stays on the device until you save it. A change entered
wrong is corrected, not added to. Schedules read these stops but never change them.

**Server:** needed.

## Read next

[How it works](how-it-works.md) follows one stop display from
start-up to the departures on its screen.
