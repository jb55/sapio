// Copyright Judica, Inc 2021
//
// This Source Code Form is subject to the terms of the Mozilla Public
//  License, v. 2.0. If a copy of the MPL was not distributed with this
//  file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Some basic examples showing a kitchen sink of functionality
use super::*;
use sapio::contract::actions::ConditionalCompileType;
use sapio_base::timelocks::RelTime;
use std::collections::LinkedList;
use std::convert::TryFrom;
use std::marker::PhantomData;

#[derive(JsonSchema, Serialize, Deserialize)]
struct ExampleA {
    alice: bitcoin::PublicKey,
    bob: bitcoin::PublicKey,
    amount: CoinAmount,
    resolution: Compiled,
}

impl ExampleA {
    guard!(timeout | s, ctx | { Clause::Older(100) });
    guard!(cached signed |s, ctx| {Clause::And(vec![Clause::Key(s.alice), Clause::Key(s.bob)])});
}

impl Contract for ExampleA {
    declare! {finish, Self::signed, Self::timeout}
    declare! {non updatable}
}

trait BState: JsonSchema {
    fn get_n(_n: u8, max: u8) -> u8 {
        return max;
    }
}
#[derive(JsonSchema, Serialize, Deserialize)]
struct Start;
impl BState for Start {}
#[derive(JsonSchema, Serialize, Deserialize)]
struct Finish;
impl BState for Finish {
    fn get_n(n: u8, _max: u8) -> u8 {
        return n;
    }
}

trait ExampleBThen
where
    Self: Sized,
{
    then! {begin_contest}
}

#[derive(JsonSchema, Serialize, Deserialize)]
struct ExampleB<T: BState> {
    participants: Vec<bitcoin::PublicKey>,
    threshold: u8,
    amount: CoinAmount,
    #[serde(skip)]
    pd: PhantomData<T>,
}

impl<T: BState> ExampleB<T> {
    guard!(cached all_signed |s, ctx| {Clause::Threshold(T::get_n(s.threshold, s.participants.len()as u8) as usize, s.participants.iter().map(|k| Clause::Key(*k)).collect())});
}

impl ExampleBThen for ExampleB<Finish> {}
impl ExampleBThen for ExampleB<Start> {
    then! {begin_contest |s, ctx| {
        ctx.template().add_output(
            s.amount.try_into()?,
            &ExampleB::<Finish> {
                participants: s.participants.clone(),
                threshold: s.threshold,
                amount: s.amount,
                pd: Default::default(),
            },
            None,
        )?.into()
    }}
}

impl<T: BState> Contract for ExampleB<T>
where
    ExampleB<T>: ExampleBThen + 'static,
{
    declare! {then, Self::begin_contest}
    declare! {finish, Self::all_signed}
    declare! {non updatable }
}

/// Trustless Escrowing Contract
#[derive(JsonSchema, Serialize, Deserialize)]
pub struct ExampleCompileIf {
    alice: bitcoin::PublicKey,
    bob: bitcoin::PublicKey,
    alice_escrow: (CoinAmount, bitcoin::Address),
    bob_escrow: (CoinAmount, bitcoin::Address),
    escrow_disable: bool,
    escrow_required_no_conflict_disabled: bool,
    escrow_required_conflict_disabled: bool,
    escrow_nullable: bool,
    escrow_error: Option<String>,
}

impl ExampleCompileIf {
    guard!(
        cooperate | s,
        ctx | { Clause::And(vec![Clause::Key(s.alice), Clause::Key(s.bob)]) }
    );
    compile_if!(
        /// `should_escrow` disables any branch depending on it. If not set,
        /// it checks to make the branch required. This is done in a conflict-free way;
        /// that is that  if escrow_required_no_conflict_disabled is set and escrow_disable
        /// is set there is no problem.
        should_escrow
            | s,
        ctx | {
            if s.escrow_disable {
                ConditionalCompileType::Never
            } else {
                if s.escrow_required_no_conflict_disabled {
                    ConditionalCompileType::Required
                } else {
                    ConditionalCompileType::Skippable
                }
            }
        }
    );
    compile_if!(
        /// `must_escrow` requires that any depending branch be taken.
        /// It may conflict with escrow_disable, if they are both set then
        /// compilation will fail.
        must_escrow
            | s,
        ctx | {
            if s.escrow_required_conflict_disabled {
                ConditionalCompileType::Required
            } else {
                ConditionalCompileType::NoConstraint
            }
        }
    );
    compile_if!(
        /// `escrow_nullable_ok` tells the compiler if it is OK if dependents on this
        /// condition return 0 txiter items -- if so, the entire branch is pruned.
        escrow_nullable_ok
            | s,
        ctx | {
            if s.escrow_nullable {
                ConditionalCompileType::Nullable
            } else {
                ConditionalCompileType::NoConstraint
            }
        }
    );

    compile_if!(
        /// `escrow_error_chk` fails with the provided error, if any
        escrow_error_chk
            | s,
        ctx | {
            if let Some(e) = &s.escrow_error {
                let mut l = LinkedList::new();
                l.push_front(e.clone());
                ConditionalCompileType::Fail(l)
            } else {
                ConditionalCompileType::NoConstraint
            }
        }
    );
    then! {use_escrow [Self::should_escrow, Self::must_escrow, Self::escrow_nullable_ok, Self::escrow_error_chk] [] |s, ctx| {
        ctx.template()
            .add_output(
                s.alice_escrow.0.try_into()?,
                &Compiled::from_address(s.alice_escrow.1.clone(), None),
                None)?
            .add_output(
                s.bob_escrow.0.try_into()?,
                &Compiled::from_address(s.bob_escrow.1.clone(), None),
                None)?
            .set_sequence(0, RelTime::try_from(std::time::Duration::from_secs(10*24*60*60))?.into())?.into()
    }}
}

impl Contract for ExampleCompileIf {
    declare! {finish, Self::cooperate}
    declare! {then, Self::use_escrow}
    declare! {non updatable}
}
