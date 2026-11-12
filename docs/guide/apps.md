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
compare with Service design. It follows the steps of the work in
order: set up the plan and its calendar (which service runs on which
days, holidays included), scan how often each route runs hour by
hour, change one route's day, check the buses and hours the change
needs, and review the whole proposal for weekdays, Saturdays and
Sundays.

**Server:** needed.

## Read next

[How it works](how-it-works.md) follows one stop display from
start-up to the departures on its screen.
