/**
 * This code was AUTOGENERATED using the kinobi library.
 * Please DO NOT EDIT THIS FILE, instead use visitors
 * to add features, then rerun kinobi to update it.
 *
 * @see https://github.com/metaplex-foundation/kinobi
 */

import {
  Account,
  Context,
  Option,
  PublicKey,
  RpcAccount,
  Serializer,
  assertAccountExists,
  deserializeAccount,
} from '@lorisleiva/js-core';
import { Key, getKeySerializer } from '../types';

export type MasterEditionV1 = Account<MasterEditionV1AccountData>;

export type MasterEditionV1AccountData = {
  key: Key;
  supply: bigint;
  maxSupply: Option<bigint>;
  printingMint: PublicKey;
  oneTimePrintingAuthorizationMint: PublicKey;
};

export type MasterEditionV1AccountArgs = {
  key: Key;
  supply: number | bigint;
  maxSupply: Option<number | bigint>;
  printingMint: PublicKey;
  oneTimePrintingAuthorizationMint: PublicKey;
};

export async function fetchMasterEditionV1(
  context: Pick<Context, 'rpc' | 'serializer'>,
  publicKey: PublicKey
): Promise<MasterEditionV1> {
  const maybeAccount = await context.rpc.getAccount(publicKey);
  assertAccountExists(maybeAccount, 'MasterEditionV1');
  return deserializeMasterEditionV1(context, maybeAccount);
}

export async function safeFetchMasterEditionV1(
  context: Pick<Context, 'rpc' | 'serializer'>,
  publicKey: PublicKey
): Promise<MasterEditionV1 | null> {
  const maybeAccount = await context.rpc.getAccount(publicKey);
  return maybeAccount.exists
    ? deserializeMasterEditionV1(context, maybeAccount)
    : null;
}

export function deserializeMasterEditionV1(
  context: Pick<Context, 'serializer'>,
  rawAccount: RpcAccount
): MasterEditionV1 {
  return deserializeAccount(
    rawAccount,
    getMasterEditionV1AccountDataSerializer(context)
  );
}

export function getMasterEditionV1AccountDataSerializer(
  context: Pick<Context, 'serializer'>
): Serializer<MasterEditionV1AccountArgs, MasterEditionV1AccountData> {
  const s = context.serializer;
  return s.struct<MasterEditionV1AccountData>(
    [
      ['key', getKeySerializer(context)],
      ['supply', s.u64],
      ['maxSupply', s.option(s.u64)],
      ['printingMint', s.publicKey],
      ['oneTimePrintingAuthorizationMint', s.publicKey],
    ],
    'MasterEditionV1'
  ) as Serializer<MasterEditionV1AccountArgs, MasterEditionV1AccountData>;
}

export function getMasterEditionV1Size(
  context: Pick<Context, 'serializer'>
): number | null {
  return getMasterEditionV1AccountDataSerializer(context).fixedSize;
}
