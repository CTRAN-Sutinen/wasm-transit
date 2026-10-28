//! Everything a feed carries beside its trips: operators, the
//! publisher, attributions, translations, stops and the parts of
//! stations, levels, pathways, transfer rules, fares, and the
//! demand-responsive catalog (location groups, zones, booking
//! rules). Each family reads into its model home and writes back from
//! it, value for value.

use std::collections::BTreeMap;

use transit_network::demand::{BookingKind, BookingRule, LocationGroup};
use transit_network::fares::{
    DurationLimit, FareLegJoinRule, FareLegRule, FareMedia, FareMediaKind, FareNetwork, FareOffer,
    FareTransferKind, FareTransferRule, RiderCategory, Timeframe, ZoneFare, ZoneFareRule,
};
use transit_network::operators::{Attribution, AttributionScope, Operator, Publisher, Translation};
use transit_network::places::{
    Level, Pathway, PathwayMode, StationPart, StationPartKind, StopInfo, StopKind, TransferEnd, TransferKind,
    TransferRule,
};
use transit_network::{Coord, Network, Stop};

use crate::feed::{
    AgencyRow, AreaRow, AttributionRow, BookingRuleRow, FareAttributeRow, FareLegJoinRuleRow, FareLegRuleRow,
    FareMediaRow, FareProductRow, FareRuleRow, FareTransferRuleRow, FeedInfoRow, LevelRow, LocationGroupRow,
    LocationGroupStopRow, NetworkRow, PathwayRow, RiderCategoryRow, StopAreaRow, StopRow, TimeframeRow,
    TransferRow, TranslationRow,
};
use crate::values::*;
use crate::{Boundary, Feed};

/// The id given an operator the feed names without one (a single
/// agency may omit agency_id).
const ANONYMOUS_OPERATOR: &str = "operator";

// ---------------------------------------------------------------------
// Operators, the publisher, attributions, translations.
// ---------------------------------------------------------------------

pub(crate) fn import_operators(feed: &Feed, net: &mut Network, boundary: &mut Boundary) {
    for row in &feed.agency {
        let id = match &row.agency_id {
            Some(id) => id.clone(),
            None => {
                boundary.anonymous_operator = Some(ANONYMOUS_OPERATOR.to_string());
                ANONYMOUS_OPERATOR.to_string()
            }
        };
        net.operators.insert(
            id,
            Operator {
                name: row.agency_name.clone().unwrap_or_default(),
                url: row.agency_url.clone().unwrap_or_default(),
                timezone: row.agency_timezone.clone().unwrap_or_default(),
                language: text(&row.agency_lang),
                phone: text(&row.agency_phone),
                fare_url: text(&row.agency_fare_url),
                email: text(&row.agency_email),
                contactless: yes_no(&row.cemv_support),
            },
        );
    }
    net.publisher = feed.feed_info.first().map(|row| Publisher {
        name: row.feed_publisher_name.clone().unwrap_or_default(),
        url: row.feed_publisher_url.clone().unwrap_or_default(),
        language: row.feed_lang.clone().unwrap_or_default(),
        default_language: text(&row.default_lang),
        start_date: text(&row.feed_start_date),
        end_date: text(&row.feed_end_date),
        version: text(&row.feed_version),
        contact_email: text(&row.feed_contact_email),
        contact_url: text(&row.feed_contact_url),
    });
    net.attributions = feed
        .attributions
        .iter()
        .map(|row| Attribution {
            id: text(&row.attribution_id),
            scope: row
                .agency_id
                .clone()
                .map(AttributionScope::Operator)
                .or_else(|| row.route_id.clone().map(AttributionScope::Route))
                .or_else(|| row.trip_id.clone().map(AttributionScope::Trip)),
            organization: row.organization_name.clone().unwrap_or_default(),
            producer: flag(&row.is_producer),
            operator: flag(&row.is_operator),
            authority: flag(&row.is_authority),
            url: text(&row.attribution_url),
            email: text(&row.attribution_email),
            phone: text(&row.attribution_phone),
        })
        .collect();
    net.translations = feed
        .translations
        .iter()
        .map(|row| Translation {
            table: row.table_name.clone().unwrap_or_default(),
            field: row.field_name.clone().unwrap_or_default(),
            language: row.language.clone().unwrap_or_default(),
            text: row.translation.clone().unwrap_or_default(),
            record: text(&row.record_id),
            sub_record: text(&row.record_sub_id),
            value: text(&row.field_value),
        })
        .collect();
}

pub(crate) fn export_operators(net: &Network, boundary: &Boundary, feed: &mut Feed) {
    if net.operators.is_empty() {
        // An authored network names no operator: the network is one.
        feed.agency.push(AgencyRow { agency_name: Some(net.name.clone()), ..Default::default() });
    }
    for (id, o) in &net.operators {
        feed.agency.push(AgencyRow {
            agency_id: (boundary.anonymous_operator.as_deref() != Some(id)).then(|| id.clone()),
            agency_name: Some(o.name.clone()),
            agency_url: Some(o.url.clone()),
            agency_timezone: Some(o.timezone.clone()),
            agency_lang: o.language.clone(),
            agency_phone: o.phone.clone(),
            agency_fare_url: o.fare_url.clone(),
            agency_email: o.email.clone(),
            cemv_support: yes_no_cell(o.contactless),
        });
    }
    if let Some(p) = &net.publisher {
        feed.feed_info.push(FeedInfoRow {
            feed_publisher_name: Some(p.name.clone()),
            feed_publisher_url: Some(p.url.clone()),
            feed_lang: Some(p.language.clone()),
            default_lang: p.default_language.clone(),
            feed_start_date: p.start_date.clone(),
            feed_end_date: p.end_date.clone(),
            feed_version: p.version.clone(),
            feed_contact_email: p.contact_email.clone(),
            feed_contact_url: p.contact_url.clone(),
        });
    }
    for a in &net.attributions {
        let scoped = |want: fn(&AttributionScope) -> Option<&String>| a.scope.as_ref().and_then(want).cloned();
        feed.attributions.push(AttributionRow {
            attribution_id: a.id.clone(),
            agency_id: scoped(|s| match s {
                AttributionScope::Operator(id) => Some(id),
                _ => None,
            }),
            route_id: scoped(|s| match s {
                AttributionScope::Route(id) => Some(id),
                _ => None,
            }),
            trip_id: scoped(|s| match s {
                AttributionScope::Trip(id) => Some(id),
                _ => None,
            }),
            organization_name: Some(a.organization.clone()),
            is_producer: flag_cell(a.producer),
            is_operator: flag_cell(a.operator),
            is_authority: flag_cell(a.authority),
            attribution_url: a.url.clone(),
            attribution_email: a.email.clone(),
            attribution_phone: a.phone.clone(),
        });
    }
    for t in &net.translations {
        feed.translations.push(TranslationRow {
            table_name: Some(t.table.clone()),
            field_name: Some(t.field.clone()),
            language: Some(t.language.clone()),
            translation: Some(t.text.clone()),
            record_id: t.record.clone(),
            record_sub_id: t.sub_record.clone(),
            field_value: t.value.clone(),
        });
    }
}

// ---------------------------------------------------------------------
// Stops, station parts, levels, pathways, transfer rules.
// ---------------------------------------------------------------------

fn stop_info(row: &StopRow) -> StopInfo {
    StopInfo {
        code: text(&row.stop_code),
        spoken_name: text(&row.tts_stop_name),
        description: text(&row.stop_desc),
        url: text(&row.stop_url),
        fare_zone: text(&row.zone_id),
        wheelchair: yes_no(&row.wheelchair_boarding),
        platform: text(&row.platform_code),
        level: text(&row.level_id),
        timezone: text(&row.stop_timezone),
        street_access: opt_bit(&row.stop_access),
    }
}

fn info_cells(info: &StopInfo, row: &mut StopRow) {
    row.stop_code = info.code.clone();
    row.tts_stop_name = info.spoken_name.clone();
    row.stop_desc = info.description.clone();
    row.stop_url = info.url.clone();
    row.zone_id = info.fare_zone.clone();
    row.wheelchair_boarding = yes_no_cell(info.wheelchair);
    row.platform_code = info.platform.clone();
    row.level_id = info.level.clone();
    row.stop_timezone = info.timezone.clone();
    row.stop_access = opt_bit_cell(info.street_access);
}

/// Stops and stations into `stops` (they need a place on the map —
/// one without coordinates is left out); entrances, nodes, and
/// boarding areas into `station_parts`.
pub(crate) fn import_places(feed: &Feed, net: &mut Network) {
    for row in &feed.stops {
        let Some(id) = &row.stop_id else { continue };
        let at = match (parse_f64(&row.stop_lat), parse_f64(&row.stop_lon)) {
            (Some(lat), Some(lon)) => Some(Coord { lat, lon }),
            _ => None,
        };
        let part = match int::<u8>(&row.location_type).unwrap_or(0) {
            2 => Some(StationPartKind::Entrance),
            3 => Some(StationPartKind::Node),
            4 => Some(StationPartKind::BoardingArea),
            _ => None,
        };
        if let Some(kind) = part {
            net.station_parts.insert(
                id.clone(),
                StationPart {
                    kind,
                    parent: row.parent_station.clone().unwrap_or_default(),
                    name: text(&row.stop_name),
                    at,
                    info: stop_info(row),
                },
            );
            continue;
        }
        // Missing coordinates: the model cannot represent a place at
        // (0, 0), and silently mapping one there poisons every map.
        let Some(at) = at else { continue };
        net.stops.insert(
            id.clone(),
            Stop {
                name: row.stop_name.clone().unwrap_or_default(),
                lat: at.lat,
                lon: at.lon,
                parent: row.parent_station.clone(),
                transfer: false,
                kind: if int::<u8>(&row.location_type) == Some(1) { StopKind::Station } else { StopKind::Stop },
                info: stop_info(row),
            },
        );
    }
    for row in &feed.levels {
        let Some(id) = &row.level_id else { continue };
        net.levels.insert(
            id.clone(),
            Level { index: parse_f64(&row.level_index).unwrap_or(0.0), name: text(&row.level_name) },
        );
    }
    for row in &feed.pathways {
        let Some(id) = &row.pathway_id else { continue };
        let mode = match int::<u8>(&row.pathway_mode) {
            Some(2) => PathwayMode::Stairs,
            Some(3) => PathwayMode::MovingSidewalk,
            Some(4) => PathwayMode::Escalator,
            Some(5) => PathwayMode::Elevator,
            Some(6) => PathwayMode::FareGate,
            Some(7) => PathwayMode::ExitGate,
            _ => PathwayMode::Walkway,
        };
        net.pathways.insert(
            id.clone(),
            Pathway {
                from: row.from_stop_id.clone().unwrap_or_default(),
                to: row.to_stop_id.clone().unwrap_or_default(),
                mode,
                both_ways: flag(&row.is_bidirectional),
                length: parse_f64(&row.length),
                traversal_secs: int(&row.traversal_time),
                stair_count: int(&row.stair_count),
                max_slope: parse_f64(&row.max_slope),
                min_width: parse_f64(&row.min_width),
                signposted_as: text(&row.signposted_as),
                reversed_signposted_as: text(&row.reversed_signposted_as),
            },
        );
    }
    net.transfers = feed
        .transfers
        .iter()
        .map(|row| TransferRule {
            from: TransferEnd {
                stop: text(&row.from_stop_id),
                route: text(&row.from_route_id),
                trip: text(&row.from_trip_id),
            },
            to: TransferEnd { stop: text(&row.to_stop_id), route: text(&row.to_route_id), trip: text(&row.to_trip_id) },
            kind: match int::<u8>(&row.transfer_type) {
                Some(1) => TransferKind::Timed,
                Some(2) => TransferKind::MinimumTime,
                Some(3) => TransferKind::NotPossible,
                Some(4) => TransferKind::InSeat,
                Some(5) => TransferKind::InSeatNotAllowed,
                _ => TransferKind::Recommended,
            },
            min_transfer_secs: int(&row.min_transfer_time),
        })
        .collect();
}

pub(crate) fn export_places(net: &Network, feed: &mut Feed) {
    // A stop other stops name as their parent is a station (an
    // authored network may group without saying so).
    let parents: std::collections::BTreeSet<&str> = net.stops.values().filter_map(|st| st.parent.as_deref()).collect();
    for (id, st) in &net.stops {
        let station = st.kind == StopKind::Station || parents.contains(id.as_str());
        let mut row = StopRow {
            stop_id: Some(id.clone()),
            stop_name: Some(st.name.clone()),
            stop_lat: Some(st.lat.to_string()),
            stop_lon: Some(st.lon.to_string()),
            location_type: station.then(|| "1".to_string()),
            parent_station: st.parent.clone(),
            ..Default::default()
        };
        info_cells(&st.info, &mut row);
        feed.stops.push(row);
    }
    for (id, part) in &net.station_parts {
        let mut row = StopRow {
            stop_id: Some(id.clone()),
            stop_name: part.name.clone(),
            stop_lat: part.at.map(|c| c.lat.to_string()),
            stop_lon: part.at.map(|c| c.lon.to_string()),
            location_type: Some(
                match part.kind {
                    StationPartKind::Entrance => "2",
                    StationPartKind::Node => "3",
                    StationPartKind::BoardingArea => "4",
                }
                .to_string(),
            ),
            parent_station: Some(part.parent.clone()),
            ..Default::default()
        };
        info_cells(&part.info, &mut row);
        feed.stops.push(row);
    }
    for (id, level) in &net.levels {
        feed.levels.push(LevelRow {
            level_id: Some(id.clone()),
            level_index: Some(level.index.to_string()),
            level_name: level.name.clone(),
        });
    }
    for (id, p) in &net.pathways {
        let mode = match p.mode {
            PathwayMode::Walkway => 1,
            PathwayMode::Stairs => 2,
            PathwayMode::MovingSidewalk => 3,
            PathwayMode::Escalator => 4,
            PathwayMode::Elevator => 5,
            PathwayMode::FareGate => 6,
            PathwayMode::ExitGate => 7,
        };
        feed.pathways.push(PathwayRow {
            pathway_id: Some(id.clone()),
            from_stop_id: Some(p.from.clone()),
            to_stop_id: Some(p.to.clone()),
            pathway_mode: Some(mode.to_string()),
            is_bidirectional: bit_cell(p.both_ways),
            length: float_cell(p.length),
            traversal_time: int_cell(p.traversal_secs),
            stair_count: int_cell(p.stair_count),
            max_slope: float_cell(p.max_slope),
            min_width: float_cell(p.min_width),
            signposted_as: p.signposted_as.clone(),
            reversed_signposted_as: p.reversed_signposted_as.clone(),
        });
    }
    for t in &net.transfers {
        let kind = match t.kind {
            TransferKind::Recommended => 0,
            TransferKind::Timed => 1,
            TransferKind::MinimumTime => 2,
            TransferKind::NotPossible => 3,
            TransferKind::InSeat => 4,
            TransferKind::InSeatNotAllowed => 5,
        };
        feed.transfers.push(TransferRow {
            from_stop_id: t.from.stop.clone(),
            to_stop_id: t.to.stop.clone(),
            from_route_id: t.from.route.clone(),
            to_route_id: t.to.route.clone(),
            from_trip_id: t.from.trip.clone(),
            to_trip_id: t.to.trip.clone(),
            transfer_type: Some(kind.to_string()),
            min_transfer_time: int_cell(t.min_transfer_secs),
        });
    }
}

// ---------------------------------------------------------------------
// Fares.
// ---------------------------------------------------------------------

pub(crate) fn import_fares(feed: &Feed, net: &mut Network) {
    let fares = &mut net.fares;
    for row in &feed.fare_attributes {
        let Some(id) = &row.fare_id else { continue };
        fares.zone_fares.insert(
            id.clone(),
            ZoneFare {
                price: parse_f64(&row.price).unwrap_or(0.0),
                currency: row.currency_type.clone().unwrap_or_default(),
                paid_before_boarding: flag(&row.payment_method),
                transfers: int(&row.transfers),
                operator: text(&row.agency_id),
                transfer_secs: int(&row.transfer_duration),
            },
        );
    }
    fares.zone_fare_rules = feed
        .fare_rules
        .iter()
        .map(|row| ZoneFareRule {
            fare: row.fare_id.clone().unwrap_or_default(),
            route: text(&row.route_id),
            origin_zone: text(&row.origin_id),
            destination_zone: text(&row.destination_id),
            contains_zone: text(&row.contains_id),
        })
        .collect();
    for row in &feed.areas {
        let Some(id) = &row.area_id else { continue };
        fares.areas.entry(id.clone()).or_default().name = text(&row.area_name);
    }
    for row in &feed.stop_areas {
        let (Some(area), Some(stop)) = (&row.area_id, &row.stop_id) else { continue };
        fares.areas.entry(area.clone()).or_default().stops.push(stop.clone());
    }
    for row in &feed.networks {
        let Some(id) = &row.network_id else { continue };
        fares.networks.insert(id.clone(), FareNetwork { name: text(&row.network_name) });
    }
    for row in &feed.timeframes {
        let Some(id) = &row.timeframe_group_id else { continue };
        fares.timeframes.entry(id.clone()).or_default().push(Timeframe {
            service_type: row.service_id.clone().unwrap_or_default(),
            start_secs: time(&row.start_time),
            end_secs: time(&row.end_time),
        });
    }
    for row in &feed.rider_categories {
        let Some(id) = &row.rider_category_id else { continue };
        fares.rider_categories.insert(
            id.clone(),
            RiderCategory {
                name: row.rider_category_name.clone().unwrap_or_default(),
                default: flag(&row.is_default_fare_category),
                eligibility_url: text(&row.eligibility_url),
            },
        );
    }
    for row in &feed.fare_media {
        let Some(id) = &row.fare_media_id else { continue };
        let kind = match int::<u8>(&row.fare_media_type) {
            Some(1) => FareMediaKind::PaperTicket,
            Some(2) => FareMediaKind::TransitCard,
            Some(3) => FareMediaKind::Contactless,
            Some(4) => FareMediaKind::MobileApp,
            _ => FareMediaKind::None,
        };
        fares.media.insert(id.clone(), FareMedia { name: text(&row.fare_media_name), kind });
    }
    for row in &feed.fare_products {
        let Some(id) = &row.fare_product_id else { continue };
        fares.products.entry(id.clone()).or_default().push(FareOffer {
            name: text(&row.fare_product_name),
            rider_category: text(&row.rider_category_id),
            media: text(&row.fare_media_id),
            amount: parse_f64(&row.amount).unwrap_or(0.0),
            currency: row.currency.clone().unwrap_or_default(),
        });
    }
    fares.leg_rules = feed
        .fare_leg_rules
        .iter()
        .map(|row| FareLegRule {
            leg_group: text(&row.leg_group_id),
            network: text(&row.network_id),
            from_area: text(&row.from_area_id),
            to_area: text(&row.to_area_id),
            from_timeframe: text(&row.from_timeframe_group_id),
            to_timeframe: text(&row.to_timeframe_group_id),
            product: row.fare_product_id.clone().unwrap_or_default(),
            priority: int::<u32>(&row.rule_priority).filter(|p| *p != 0),
        })
        .collect();
    fares.leg_join_rules = feed
        .fare_leg_join_rules
        .iter()
        .map(|row| FareLegJoinRule {
            from_network: row.from_network_id.clone().unwrap_or_default(),
            to_network: row.to_network_id.clone().unwrap_or_default(),
            from_stop: text(&row.from_stop_id),
            to_stop: text(&row.to_stop_id),
        })
        .collect();
    fares.transfer_rules = feed
        .fare_transfer_rules
        .iter()
        .map(|row| FareTransferRule {
            from_leg_group: text(&row.from_leg_group_id),
            to_leg_group: text(&row.to_leg_group_id),
            transfer_count: int(&row.transfer_count),
            duration_limit_secs: int(&row.duration_limit),
            duration_limit: match int::<u8>(&row.duration_limit_type) {
                Some(0) => Some(DurationLimit::DepartureToArrival),
                Some(1) => Some(DurationLimit::DepartureToDeparture),
                Some(2) => Some(DurationLimit::ArrivalToDeparture),
                Some(3) => Some(DurationLimit::ArrivalToArrival),
                _ => None,
            },
            kind: match int::<u8>(&row.fare_transfer_type) {
                Some(1) => FareTransferKind::EveryLegPlusTransfer,
                Some(2) => FareTransferKind::TransferOnly,
                _ => FareTransferKind::FirstLegPlusTransfer,
            },
            product: text(&row.fare_product_id),
        })
        .collect();
}

pub(crate) fn export_fares(net: &Network, feed: &mut Feed) {
    let fares = &net.fares;
    for (id, f) in &fares.zone_fares {
        feed.fare_attributes.push(FareAttributeRow {
            fare_id: Some(id.clone()),
            price: Some(f.price.to_string()),
            currency_type: Some(f.currency.clone()),
            payment_method: bit_cell(f.paid_before_boarding),
            transfers: int_cell(f.transfers),
            agency_id: f.operator.clone(),
            transfer_duration: int_cell(f.transfer_secs),
        });
    }
    for r in &fares.zone_fare_rules {
        feed.fare_rules.push(FareRuleRow {
            fare_id: Some(r.fare.clone()),
            route_id: r.route.clone(),
            origin_id: r.origin_zone.clone(),
            destination_id: r.destination_zone.clone(),
            contains_id: r.contains_zone.clone(),
        });
    }
    for (id, area) in &fares.areas {
        feed.areas.push(AreaRow { area_id: Some(id.clone()), area_name: area.name.clone() });
        for stop in &area.stops {
            feed.stop_areas.push(StopAreaRow { area_id: Some(id.clone()), stop_id: Some(stop.clone()) });
        }
    }
    for (id, n) in &fares.networks {
        feed.networks.push(NetworkRow { network_id: Some(id.clone()), network_name: n.name.clone() });
    }
    for (id, spans) in &fares.timeframes {
        for t in spans {
            feed.timeframes.push(TimeframeRow {
                timeframe_group_id: Some(id.clone()),
                start_time: time_cell(t.start_secs),
                end_time: time_cell(t.end_secs),
                service_id: Some(t.service_type.clone()),
            });
        }
    }
    for (id, c) in &fares.rider_categories {
        feed.rider_categories.push(RiderCategoryRow {
            rider_category_id: Some(id.clone()),
            rider_category_name: Some(c.name.clone()),
            is_default_fare_category: flag_cell(c.default),
            eligibility_url: c.eligibility_url.clone(),
        });
    }
    for (id, m) in &fares.media {
        let kind = match m.kind {
            FareMediaKind::None => 0,
            FareMediaKind::PaperTicket => 1,
            FareMediaKind::TransitCard => 2,
            FareMediaKind::Contactless => 3,
            FareMediaKind::MobileApp => 4,
        };
        feed.fare_media.push(FareMediaRow {
            fare_media_id: Some(id.clone()),
            fare_media_name: m.name.clone(),
            fare_media_type: Some(kind.to_string()),
        });
    }
    for (id, offers) in &fares.products {
        for o in offers {
            feed.fare_products.push(FareProductRow {
                fare_product_id: Some(id.clone()),
                fare_product_name: o.name.clone(),
                rider_category_id: o.rider_category.clone(),
                fare_media_id: o.media.clone(),
                amount: Some(o.amount.to_string()),
                currency: Some(o.currency.clone()),
            });
        }
    }
    for r in &fares.leg_rules {
        feed.fare_leg_rules.push(FareLegRuleRow {
            leg_group_id: r.leg_group.clone(),
            network_id: r.network.clone(),
            from_area_id: r.from_area.clone(),
            to_area_id: r.to_area.clone(),
            from_timeframe_group_id: r.from_timeframe.clone(),
            to_timeframe_group_id: r.to_timeframe.clone(),
            fare_product_id: Some(r.product.clone()),
            rule_priority: int_cell(r.priority),
        });
    }
    for r in &fares.leg_join_rules {
        feed.fare_leg_join_rules.push(FareLegJoinRuleRow {
            from_network_id: Some(r.from_network.clone()),
            to_network_id: Some(r.to_network.clone()),
            from_stop_id: r.from_stop.clone(),
            to_stop_id: r.to_stop.clone(),
        });
    }
    for r in &fares.transfer_rules {
        feed.fare_transfer_rules.push(FareTransferRuleRow {
            from_leg_group_id: r.from_leg_group.clone(),
            to_leg_group_id: r.to_leg_group.clone(),
            transfer_count: int_cell(r.transfer_count),
            duration_limit: int_cell(r.duration_limit_secs),
            duration_limit_type: r.duration_limit.map(|d| {
                match d {
                    DurationLimit::DepartureToArrival => "0",
                    DurationLimit::DepartureToDeparture => "1",
                    DurationLimit::ArrivalToDeparture => "2",
                    DurationLimit::ArrivalToArrival => "3",
                }
                .to_string()
            }),
            fare_transfer_type: Some(
                match r.kind {
                    FareTransferKind::FirstLegPlusTransfer => "0",
                    FareTransferKind::EveryLegPlusTransfer => "1",
                    FareTransferKind::TransferOnly => "2",
                }
                .to_string(),
            ),
            fare_product_id: r.product.clone(),
        });
    }
}

// ---------------------------------------------------------------------
// The demand-responsive catalog (its trips are read with the others).
// ---------------------------------------------------------------------

pub(crate) fn import_demand_catalog(feed: &Feed, net: &mut Network) {
    let demand = &mut net.demand;
    for row in &feed.location_groups {
        let Some(id) = &row.location_group_id else { continue };
        demand.location_groups.entry(id.clone()).or_default().name = text(&row.location_group_name);
    }
    for row in &feed.location_group_stops {
        let (Some(group), Some(stop)) = (&row.location_group_id, &row.stop_id) else { continue };
        demand.location_groups.entry(group.clone()).or_insert_with(LocationGroup::default).stops.push(stop.clone());
    }
    demand.zones = feed.locations.clone();
    for row in &feed.booking_rules {
        let Some(id) = &row.booking_rule_id else { continue };
        demand.booking_rules.insert(
            id.clone(),
            BookingRule {
                kind: match int::<u8>(&row.booking_type) {
                    Some(1) => BookingKind::SameDay,
                    Some(2) => BookingKind::PriorDays,
                    _ => BookingKind::RealTime,
                },
                notice_min_minutes: int(&row.prior_notice_duration_min),
                notice_max_minutes: int(&row.prior_notice_duration_max),
                last_day: int(&row.prior_notice_last_day),
                last_time_secs: time(&row.prior_notice_last_time),
                start_day: int(&row.prior_notice_start_day),
                start_time_secs: time(&row.prior_notice_start_time),
                booking_days: text(&row.prior_notice_service_id),
                message: text(&row.message),
                pickup_message: text(&row.pickup_message),
                drop_off_message: text(&row.drop_off_message),
                phone: text(&row.phone_number),
                info_url: text(&row.info_url),
                booking_url: text(&row.booking_url),
            },
        );
    }
}

pub(crate) fn export_demand_catalog(net: &Network, feed: &mut Feed) {
    let demand = &net.demand;
    for (id, g) in &demand.location_groups {
        feed.location_groups.push(LocationGroupRow { location_group_id: Some(id.clone()), location_group_name: g.name.clone() });
        for stop in &g.stops {
            feed.location_group_stops
                .push(LocationGroupStopRow { location_group_id: Some(id.clone()), stop_id: Some(stop.clone()) });
        }
    }
    feed.locations = demand.zones.clone();
    for (id, r) in &demand.booking_rules {
        let kind = match r.kind {
            BookingKind::RealTime => 0,
            BookingKind::SameDay => 1,
            BookingKind::PriorDays => 2,
        };
        feed.booking_rules.push(BookingRuleRow {
            booking_rule_id: Some(id.clone()),
            booking_type: Some(kind.to_string()),
            prior_notice_duration_min: int_cell(r.notice_min_minutes),
            prior_notice_duration_max: int_cell(r.notice_max_minutes),
            prior_notice_last_day: int_cell(r.last_day),
            prior_notice_last_time: time_cell(r.last_time_secs),
            prior_notice_start_day: int_cell(r.start_day),
            prior_notice_start_time: time_cell(r.start_time_secs),
            prior_notice_service_id: r.booking_days.clone(),
            message: r.message.clone(),
            pickup_message: r.pickup_message.clone(),
            drop_off_message: r.drop_off_message.clone(),
            phone_number: r.phone.clone(),
            info_url: r.info_url.clone(),
            booking_url: r.booking_url.clone(),
        });
    }
}

/// Fare networks by route: routes.txt's network_id or
/// route_networks.txt (the spec forbids both), and which one the feed
/// used.
pub(crate) fn route_fare_networks(feed: &Feed, boundary: &mut Boundary) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for row in &feed.routes {
        if let (Some(route), Some(network)) = (&row.route_id, &row.network_id) {
            out.insert(route.clone(), network.clone());
            boundary.fare_networks_on_routes = true;
        }
    }
    for row in &feed.route_networks {
        if let (Some(route), Some(network)) = (&row.route_id, &row.network_id) {
            out.insert(route.clone(), network.clone());
        }
    }
    out
}
