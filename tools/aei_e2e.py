#!/usr/bin/env python3
"""Exercise AEI in all game modes and both single-player paths.

Requires a fast build with --features mods, prepared game resources and the
SDK example built by cargo test -p horizon-mods --features runtime,pack.
Every run uses fresh temporary game, launcher and XDG homes.
"""
import argparse
import json
import math
from pathlib import Path
import subprocess
import tempfile
import time

from tools import lab

ROOT = Path(__file__).resolve().parents[1]
NATIVE_CATEGORIES = {'minecraft:crafting','minecraft:stonecutting','minecraft:smelting',
                     'minecraft:smoking','minecraft:blasting','minecraft:campfire_cooking',
                     'minecraft:smithing','minecraft:compostable','minecraft:fuel',
                     'minecraft:brewing','minecraft:anvil','minecraft:grindstone'}


def run(args, mode, output):
    folder = Path(tempfile.mkdtemp(prefix=mode + '-', dir=output))
    home = folder / 'home'
    env = lab.game_env(
        HORIZON_HOME=home,
        HORIZON_LAUNCHER_HOME=folder / 'launcher',
        XDG_DATA_HOME=folder / 'xdg-data',
        XDG_CONFIG_HOME=folder / 'xdg-config',
        HORIZON_TELEMETRY_URL='off',
    )
    for key in ('HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME'):
        Path(env[key]).mkdir(parents=True, exist_ok=True)
    (home / 'options.txt').write_text('guiScale:2\n')
    socket = folder / 'ctl.sock'
    command = [str(args.bin_dir / 'horizon-client'), '--resources', str(args.resources),
               '--world', 'AEI-API-Test', '--test-terrain', '--offscreen', '--control',
               '--control-socket', str(socket), '--render-distance', '2',
               '--width', '1280', '--height', '720', '--dev-mod', str(args.mod)]
    if mode == 'in-frame':
        command.append('--in-frame')
    log = (folder / 'game.log').open('w')
    game = subprocess.Popen(command, cwd=home, env=env, stdout=log, stderr=subprocess.STDOUT)

    def ctl(*words):
        result = subprocess.run([str(args.bin_dir / 'horizon-ctl'), '--socket', str(socket),
                                 *map(str, words)], cwd=home, env=env, text=True,
                                capture_output=True, timeout=30)
        assert result.returncode == 0, (words, result.stdout, result.stderr)
        value = json.loads(result.stdout)
        assert 'error' not in value, (words, value)
        return value

    def aei():
        return ctl('screen')['aei']

    def playing():
        for _ in range(8):
            if ctl('state')['screen'] == 'playing':
                return
            ctl('key', 'esc', 'tap')
        raise AssertionError('Could not return to play')

    def query(value):
        x, y, _, _ = aei()['layout']['search']
        ctl('click-at', x + 8, y + 10, 'right')
        if value:
            ctl('type', value)
        ctl('key', 'enter', 'tap')

    def all_categories():
        x, y, w, h = aei()['layout']['categories_button']
        ctl('click-at', x+w/2, y+h/2)
        assert set(aei()['categories']) >= NATIVE_CATEGORIES | {'aei_example:pressing'}, aei()


    def lookup(right=False):
        x, y, _, _ = aei()['layout']['catalogue']
        ctl('click-at', x + 9, y + 9, *(['right'] if right else []))

    def wait_mode(value):
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if ctl('state')['gamemode'] == value:
                return
            time.sleep(.05)
        raise AssertionError('Game mode did not change to ' + value)

    def open_block(block):
        playing()
        ctl('command', 'gamemode survival')
        wait_mode('survival')
        position = ctl('state')['player']['position']
        x, y, z = map(math.floor, position)
        # Clear a small room so trees at the test spawn cannot occlude the click.
        ctl('command', f'fill {x-3} {y} {z-3} {x+3} {y+4} {z+3} minecraft:air')
        ctl('command', f'fill {x-3} {y-1} {z-3} {x+3} {y-1} {z+3} minecraft:stone')
        x += 2
        ctl('command', f'setblock {x} {y} {z} minecraft:{block}')
        ctl('wait', 8)
        ctl('interact', x, y, z)
        assert ctl('state')['screen'] in ('inventory', 'container', 'station'), ctl('state')

    try:
        deadline = time.monotonic() + 60
        while time.monotonic() < deadline:
            assert game.poll() is None, (folder / 'game.log').read_text()[-5000:]
            try:
                state = ctl('state')
                if state.get('screen') == 'playing' and state.get('player'):
                    break
            except (AssertionError, subprocess.TimeoutExpired):
                pass
            time.sleep(.1)
        else:
            raise AssertionError('Game did not become ready')

        ctl('command', 'difficulty peaceful')
        ctl('command', 'gamerule keep_inventory true')
        ctl('give', 'minecraft:iron_ingot', 9)
        for game_mode in ('survival', 'creative', 'adventure', 'spectator'):
            playing()
            ctl('command', 'gamemode ' + game_mode)
            wait_mode(game_mode)
            ctl('open-inventory')
            browser = aei()
            assert browser['visible'], browser
            assert browser['read_only'] == (game_mode == 'spectator'), browser
            assert 'aei_example:pressing' in browser['registered_categories'], browser
            query('minecraft:iron_block')
            lookup()
            assert 'aei_example:pressing' in aei()['categories'], aei()
            ctl('key', 'esc', 'tap')
            assert aei()['view'] == 'items'
            query('minecraft:iron_ingot')
            lookup(True)
            assert 'aei_example:pressing' in aei()['categories'], aei()
            ctl('key', 'shift', 'down')
            ctl('key', 'pageup', 'tap')
            ctl('key', 'shift', 'up')
            assert aei()['category_id'] == 'aei_example:pressing', aei()
            ctl('screenshot', str(folder / (game_mode + '.png')))
            ctl('key', 'esc', 'tap')
            if game_mode == 'spectator':
                before = ctl('inventory')
                ctl('click-slot', 0)
                ctl('key', 'q', 'tap')
                assert ctl('inventory') == before, 'Spectator browser changed inventory'
            ctl('key', 'ctrl', 'down')
            ctl('key', 'r', 'tap')
            ctl('key', 'ctrl', 'up')
            assert not aei()['visible']
            ctl('key', 'ctrl', 'down')
            ctl('key', 'r', 'tap')
            ctl('key', 'ctrl', 'up')
            assert aei()['visible']
            grid_x, _, grid_w, _ = aei()['layout']['catalogue']
            assert grid_x + grid_w == 634, 'AEI must stay six GUI pixels from the right edge'
            all_categories()
            if game_mode == 'survival':
                visited = set()
                for _ in range(len(aei()['categories'])):
                    category = aei()['category_id']
                    visited.add(category)
                    assert aei()['recipe_ids'], aei()
                    visible = aei()['layout']['visible_recipes']
                    assert visible and visible[0]['slots'], aei()
                    if category == 'minecraft:brewing':
                        assert any(slot['stack'] and 'minecraft:potion_contents' in slot['stack'].get('components', '') for recipe in visible for slot in recipe['slots']), visible
                    if category in NATIVE_CATEGORIES - {'minecraft:crafting','minecraft:smelting','minecraft:blasting','minecraft:smoking'}:
                        ctl('click-at', 5, 5)
                        ctl('screenshot', str(folder / (category.split(':')[1] + '.png')))
                    ctl('key', 'shift', 'down')
                    ctl('key', 'pagedown', 'tap')
                    ctl('key', 'shift', 'up')
                assert visited >= NATIVE_CATEGORIES, visited
            ctl('key', 'esc', 'tap')

        playing()
        ctl('command', 'clear @s')
        ctl('give', 'minecraft:oak_log', 1)
        ctl('wait', 2)
        # Native arrows open every recipe in the category and preserve the menu.
        for block, category, point in (
                ('crafting_table', 'minecraft:crafting', (100, 42)),
                ('furnace', 'minecraft:smelting', (90, 42))):
            open_block(block)
            query('')
            expected = 'minecraft:oak_planks' if block == 'crafting_table' else 'minecraft:charcoal'
            assert aei()['items'][0] == expected, aei()
            assert category in aei()['context_categories'], aei()
            original = ctl('screen')
            x, y = aei()['layout']['container_origin']
            ctl('click-at', x + point[0], y + point[1])
            assert aei()['category_id'] == category, aei()
            assert len(aei()['recipe_ids']) > 1
            assert aei()['recipe_ids'][0] == expected, aei()
            assert expected in aei()['craftable_recipes'], aei()
            # Category-title clicks expand a station/item lookup to the full registry.
            area_x, area_y, area_w, _ = aei()['layout']['recipes']
            ctl('click-at', area_x+area_w/2, area_y+9)
            assert 'aei_example:pressing' in aei()['categories'], aei()
            assert aei()['category_id'] == category, aei()
            if block == 'crafting_table':
                ctl('give', 'minecraft:oak_planks', 4)
                ctl('wait', 2)
                assert 'minecraft:crafting_table' in aei()['craftable_recipes'], aei()
            ctl('key', 'esc', 'tap')
            restored = ctl('screen')
            assert restored['container'] == original['container']
            assert restored['craft_size'] == original['craft_size']
            assert aei()['view'] == 'items'

        # Additional native workstation contexts select their own categories.
        for block, category, ingredients in (
                ('stonecutter', 'minecraft:stonecutting', [('minecraft:stone', 1)]),
                ('smithing_table', 'minecraft:smithing', [('minecraft:diamond_sword', 1), ('minecraft:netherite_ingot', 1), ('minecraft:netherite_upgrade_smithing_template', 1)]),
                ('brewing_stand', 'minecraft:brewing', [('minecraft:nether_wart', 1)])):
            playing()
            ctl('command', 'clear @a')
            for item, count in ingredients:
                ctl('give', item, count)
            if block == 'brewing_stand':
                ctl('command', 'give @a minecraft:potion[minecraft:potion_contents={potion:"minecraft:water"}] 1')
            ctl('wait', 2)
            if block == 'brewing_stand':
                inventory = ctl('inventory')
                potions = [stack for stack in inventory['hotbar'] + inventory['main'] if stack.get('item') == 'minecraft:potion']
                assert potions and 'minecraft:water' in potions[0].get('components', ''), inventory
            open_block(block)
            query('')
            all_categories()
            assert aei()['category_id'] == category, aei()
            assert aei()['craftable_recipes'], {'category': aei()['category_id'], 'inventory': ctl('inventory'), 'visible': aei()['layout']['visible_recipes']}
            ctl('screenshot', str(folder / (block + '-available.png')))

        # Closing the station restores the original creative-order catalogue.
        playing()
        ctl('open-inventory')
        query('')
        assert aei()['context_categories'] == [], aei()
        assert aei()['items'][0] == 'minecraft:stone', aei()
        playing()
        ctl('command', 'clear @s')
        ctl('give', 'minecraft:iron_ingot', 8)
        ctl('wait', 2)
        # The SDK's click area and catalyst open a custom category natively.
        open_block('anvil')
        x, y = aei()['layout']['container_origin']
        ctl('click-at', x + 110, y + 55)
        assert aei()['category_id'] == 'aei_example:pressing', aei()
        assert aei()['recipe_ids'] == ['aei_example:iron_block'], aei()
        assert aei()['craftable_recipes'] == [], aei()
        ctl('give', 'minecraft:iron_ingot', 1)
        ctl('wait', 2)
        assert aei()['craftable_recipes'] == ['aei_example:iron_block'], aei()
        ctl('key', 'esc', 'tap')
        query('minecraft:iron_block')
        lookup()
        assert aei()['category_id'] == 'aei_example:pressing', aei()
        assert aei()['categories'][0] == 'aei_example:pressing', aei()
        ctl('screenshot', str(folder / 'sdk-container-integration.png'))
        ctl('key', 'esc', 'tap')
        query('minecraft:anvil')
        lookup(True)
        assert 'aei_example:pressing' in aei()['categories'], aei()
        playing()
        ctl('key', 't', 'tap')
        ctl('type', '/aei_example')
        ctl('key', 'enter', 'tap')
        assert aei()['category_id'] == 'aei_example:pressing', aei()
        playing()
        ctl('command', 'aei_example clear')
        ctl('open-inventory')
        assert 'aei_example:pressing' not in aei()['registered_categories'], aei()
        playing()
        ctl('command', 'aei_example restore')
        ctl('command', 'aei_example')
        assert aei()['category_id'] == 'aei_example:pressing', aei()
        # Horizon settings persist independently, including chords and mouse bindings.
        playing()
        ctl('options', 'horizon')
        button = next(w for w in ctl('ui')['widgets'] if w.get('label') == 'AEI Settings...')
        ctl('click', button['id'])
        def click_label(label):
            # The AEI binding list can extend below the viewport at GUI scale 2.
            ctl('scroll-at', 320, 200, 30)
            for _ in range(8):
                widgets = ctl('ui')['widgets']
                widget = next((w for w in widgets if w.get('label', '').removeprefix('[ ').removesuffix(' ]') == label
                               and w.get('enabled') and w['bounds'][3] >= 18), None)
                if widget:
                    ctl('click', widget['id'])
                    return
                ctl('scroll-at', 320, 200, -2)
            raise AssertionError((label, widgets))
        click_label('AEI: On')
        click_label('Vanilla recipe book: On')
        ctl('click-at', 5, 5)
        ctl('screenshot', str(folder / 'aei-settings-disabled.png'))
        playing()
        ctl('command', 'gamemode survival')
        wait_mode('survival')
        ctl('open-inventory')
        assert not aei()['enabled'] and not aei()['visible'], aei()
        assert not aei()['recipe_book_enabled'] and 'recipe_book' not in ctl('screen')
        ctl('key', 'ctrl', 'down')
        ctl('key', 'r', 'tap')
        ctl('key', 'ctrl', 'up')
        assert not aei()['visible'], 'Visibility shortcut must not enable a disabled AEI'
        playing()
        ctl('command', 'gamemode spectator')
        wait_mode('spectator')
        ctl('open-inventory')
        assert ctl('state')['screen'] == 'playing', 'Disabled AEI must not open a spectator browser'
        ctl('command', 'gamemode survival')
        wait_mode('survival')
        playing()
        ctl('options', 'aei')
        click_label('AEI: Off')
        # Rebind visibility while retaining the recipe book setting.
        click_label('Ctrl + R')
        ctl('key', 'ctrl', 'down')
        ctl('key', 'k', 'tap')
        ctl('key', 'ctrl', 'up')
        ctl('click-at', 5, 5)
        ctl('screenshot', str(folder / 'aei-settings-keybinds.png'))
        playing()
        ctl('open-inventory')
        assert aei()['visible'] and not aei()['recipe_book_enabled'], aei()
        assert 'recipe_book' not in ctl('screen')
        ctl('key', 'ctrl', 'down')
        ctl('key', 'r', 'tap')
        ctl('key', 'ctrl', 'up')
        assert aei()['visible'], 'Old visibility shortcut still active'
        ctl('key', 'ctrl', 'down')
        ctl('key', 'k', 'tap')
        ctl('key', 'ctrl', 'up')
        assert not aei()['visible']
        ctl('key', 'ctrl', 'down')
        ctl('key', 'k', 'tap')
        ctl('key', 'ctrl', 'up')
        assert aei()['visible']
        ctl('key', 'ctrl', 'down')
        ctl('key', 'f', 'tap')
        ctl('key', 'ctrl', 'up')
        assert aei()['focused']
        ctl('key', 'ctrl', 'down')
        ctl('key', 'a', 'tap')
        ctl('key', 'ctrl', 'up')
        ctl('type', 'minecraft:oak_planks')
        ctl('key', 'enter', 'tap')
        # AEI recipe lookup still works with the vanilla book disabled.
        x, y, _, _ = aei()['layout']['catalogue']
        ctl('click-at', x+9, y+9)
        ctl('key', 'esc', 'tap')
        ctl('key', 'r', 'tap')
        assert aei()['view'] == 'recipes', aei()
        playing()
        ctl('options', 'keybinds')
        # Search exposes the same AEI category in the normal controls list.
        search = next(w for w in ctl('ui')['widgets'] if w.get('kind') == 'field')
        ctl('click', search['id'])
        ctl('type', 'AEI')
        assert any(w.get('label') == 'Ctrl + K' for w in ctl('ui')['widgets']), ctl('ui')
        ctl('screenshot', str(folder / 'aei-controls.png'))
        playing()
        ctl('options', 'aei')
        click_label('Vanilla recipe book: Off')
        click_label('R')
        ctl('key', 'esc', 'tap')
        assert any(w.get('label') == 'Not Bound' for w in ctl('ui')['widgets'])
        click_label('Ctrl + K')
        done = next(w for w in ctl('ui')['widgets'] if w.get('label') == 'Done')
        ctl('click', done['id'], 'middle')
        assert 'horizon.aei.toggle:key.mouse.middle' in (home / 'horizon-extras.txt').read_text()
        playing()
        ctl('open-inventory')
        ctl('click', 'slot-0', 'middle')
        assert not aei()['visible']
        ctl('click', 'slot-0', 'middle')
        assert aei()['visible']
        playing()
        ctl('options', 'aei')
        click_label('Reset Keys')
        playing()
        ctl('open-inventory')
        assert aei()['keybinds']['horizon.aei.toggle'] == 'ctrl+key.keyboard.r', aei()
        assert 'recipe_book' in ctl('screen')
        saved = (home / 'horizon-extras.txt').read_text()
        assert 'aeiEnabled:true' in saved and 'recipeBookEnabled:true' in saved, saved
        # JEI transfer fills the parent menu, never takes its output automatically.
        playing()
        ctl('options', 'aei')
        click_label('Vanilla recipe book: On')
        playing()
        ctl('command', 'clear @a')
        ctl('give', 'minecraft:oak_planks', 3)
        open_block('crafting_table')
        query('minecraft:crafting_table')
        lookup()
        def transfer():
            recipe = aei()['layout']['visible_recipes'][0]
            assert recipe['transfer'] is not None, recipe
            return recipe['transfer']
        def plus(right=False):
            x,y,w,h = transfer()['area']
            ctl('click-at', x+w/2, y+h/2, *(['right'] if right else []))
        def contents():
            return {s['slot']:s for s in ctl('screen')['slots'] if s.get('item')}
        def inventory_count(item):
            inv = ctl('inventory')
            return sum(s.get('count',0) for s in inv['hotbar']+inv['main'] if s.get('item') == item)
        assert not transfer()['enabled'] and len(transfer()['missing']) == 1, transfer()
        before = ctl('inventory')
        plus()
        assert ctl('inventory') == before and aei()['view'] == 'recipes'
        ctl('screenshot', str(folder / 'transfer-missing.png'))
        ctl('give', 'minecraft:oak_planks', 1)
        ctl('wait', 2)
        assert transfer()['enabled'], transfer()
        before = ctl('inventory')
        plus(True)
        assert aei()['view'] == 'recipes' and ctl('inventory') == before, 'Right click must not transfer'
        x,y,w,h = transfer()['area']
        before = ctl('inventory')
        ctl('drag-at', x+w/2, y+h/2, 5, 5)
        assert ctl('inventory') == before and aei()['view'] == 'recipes'
        plus()
        assert aei()['view'] == 'items' and ctl('screen')['craft_size'] == 3
        slots = contents()
        assert all(slots[s]['item'] == 'minecraft:oak_planks' and slots[s]['count'] == 1 for s in (100,101,103,104)), slots
        assert slots[200]['item'] == 'minecraft:crafting_table' and inventory_count('minecraft:crafting_table') == 0, slots
        assert inventory_count('minecraft:oak_planks') == 0
        ctl('screenshot', str(folder / 'transfer-crafting-table.png'))
        ctl('click-slot', 200, 'shift')
        ctl('wait', 2)
        assert inventory_count('minecraft:crafting_table') == 1
        assert not any(s in contents() for s in range(100,109))
        ctl('give', 'minecraft:oak_planks', 8)
        query('minecraft:stick')
        lookup()
        ctl('key', 'shift', 'down')
        plus()
        ctl('key', 'shift', 'up')
        slots = contents()
        assert slots[101]['count'] == slots[104]['count'] == 4, slots
        assert slots[200]['item'] == 'minecraft:stick' and inventory_count('minecraft:stick') == 0
        ctl('click-slot', 200, 'shift')
        ctl('wait', 2)
        assert inventory_count('minecraft:stick') == 16
        # Old grid contents are returned intact before placing the new recipe.
        playing()
        ctl('command', 'clear @a')
        ctl('give', 'minecraft:dirt', 1)
        open_block('crafting_table')
        ctl('click-slot', 0)
        ctl('click-slot', 100)
        ctl('give', 'minecraft:oak_planks', 4)
        query('minecraft:crafting_table')
        lookup()
        assert transfer()['enabled'], transfer()
        plus()
        assert inventory_count('minecraft:dirt') == 1
        ctl('click-slot', 200, 'shift')
        # A full inventory never ejects old ingredients or applies half a transfer.
        playing()
        ctl('command','clear @a')
        ctl('give','minecraft:dirt',1)
        open_block('crafting_table')
        ctl('click-slot',0)
        ctl('click-slot',100)
        ctl('give','minecraft:oak_planks',4)
        ctl('give','minecraft:stone',2240)
        query('minecraft:crafting_table')
        lookup()
        assert not transfer()['enabled'] and 'too full' in transfer()['error'], transfer()
        before=ctl('inventory')
        plus()
        assert ctl('inventory') == before
        ctl('screenshot',str(folder / 'transfer-inventory-full.png'))
        ctl('command','clear @a')
        # Every transfer remains a normal input move in native workstation menus.
        for block,item,ingredients,expected in (
                ('furnace','minecraft:iron_ingot',[('minecraft:raw_iron',3)],{300:'minecraft:raw_iron'}),
                ('stonecutter','minecraft:stone_bricks',[('minecraft:stone',3)],{600:'minecraft:stone'}),
                ('smithing_table','minecraft:netherite_sword',[('minecraft:diamond_sword',1),('minecraft:netherite_upgrade_smithing_template',1),('minecraft:netherite_ingot',1)],{600:'minecraft:netherite_upgrade_smithing_template',601:'minecraft:diamond_sword',602:'minecraft:netherite_ingot'}),
                ('brewing_stand','minecraft:potion',[('minecraft:nether_wart',1)],{300:'minecraft:potion',303:'minecraft:nether_wart'}),
                ('anvil','minecraft:diamond_sword',[],{600:'minecraft:diamond_sword',601:'minecraft:diamond_sword'}),
                ('grindstone','minecraft:diamond_sword',[],{600:'minecraft:diamond_sword',601:'minecraft:diamond_sword'})):

            playing()
            ctl('command','clear @a')
            for ingredient,count in ingredients: ctl('give',ingredient,count)
            if block in ('anvil','grindstone'):
                ctl('command','give @a minecraft:diamond_sword[minecraft:damage=1000] 2')
            if block == 'brewing_stand':
                ctl('command','give @a minecraft:potion[minecraft:potion_contents={potion:"minecraft:water"}] 1')
            ctl('wait',2)
            open_block(block)
            query(item)
            lookup()
            assert transfer()['enabled'], {'block':block,'transfer':transfer(),'inventory':ctl('inventory')}
            plus()
            slots=contents()
            assert all(slots[s]['item'] == ingredient for s,ingredient in expected.items()), (block,slots)
            if block == 'stonecutter': assert slots[650]['item'] == 'minecraft:stone_bricks', slots
            if block in ('anvil','grindstone'):
                assert slots[600]['damage'] == slots[601]['damage'] == 1000, slots
                assert slots[650]['item'] == 'minecraft:diamond_sword', slots
            if block == 'brewing_stand': assert 'minecraft:water' in slots[300].get('components',''), slots
            ctl('screenshot',str(folder / ('transfer-'+block+'.png')))
        # The ordinary 2x2 inventory grid is supported with the book disabled too.
        playing()
        ctl('command','clear @a')
        ctl('give','minecraft:oak_log',2)
        ctl('open-inventory')
        query('minecraft:oak_planks')
        lookup()
        assert transfer()['enabled'], transfer()
        plus()
        assert contents()[100]['item'] == 'minecraft:oak_log'
        assert contents()[200]['item'] == 'minecraft:oak_planks'
        assert inventory_count('minecraft:oak_planks') == 0
        ctl('click-slot',200,'shift')
        ctl('wait',2)
        assert inventory_count('minecraft:oak_planks') == 4
        ctl('click-slot',0)
        query('minecraft:oak_planks')
        lookup()
        assert not transfer()['enabled'] and 'carried' in transfer()['error'], transfer()
        ctl('key','esc','tap')
        ctl('click-slot',0)
        # Shapeless ingredients follow the viewer's compact layout.
        playing()
        ctl('command','clear @a')
        for item in ('minecraft:coal','minecraft:blaze_powder','minecraft:gunpowder'): ctl('give',item,1)
        open_block('crafting_table')
        query('minecraft:fire_charge')
        lookup()
        assert transfer()['enabled'], transfer()
        plus()
        assert contents()[200]['item'] == 'minecraft:fire_charge', contents()
        assert all(slot in contents() for slot in (100,101,103)), contents()
        ctl('click-slot',200,'shift')
        # An incompatible 2x2 grid and spectator mode cannot move items.
        playing()
        ctl('command','clear @a')
        ctl('give','minecraft:oak_planks',5)
        ctl('open-inventory')
        query('minecraft:oak_boat')
        lookup()
        assert not transfer()['enabled'] and 'larger crafting grid' in transfer()['error'], transfer()
        playing()
        ctl('command','gamemode spectator')
        wait_mode('spectator')
        ctl('open-inventory')
        query('minecraft:crafting_table')
        lookup()
        assert not transfer()['enabled'] and 'read-only' in transfer()['error'], transfer()
        before=ctl('inventory')
        plus()
        assert ctl('inventory') == before
        # A tag's alternatives pause while held, step in both directions and resume.
        def recipe_stacks():
            return [s['stack'] for s in aei()['layout']['visible_recipes'][0]['slots']]
        def hover_ingredient():
            slot = next(s for s in aei()['layout']['visible_recipes'][0]['slots'] if s['stack'])
            x,y,_,_ = slot['area']
            ctl('scroll-at', x+8, y+8, 0)
            return x+8,y+8
        x,y = hover_ingredient()
        ctl('key','shift','down')
        original = recipe_stacks()
        assert aei()['cycle_paused']
        time.sleep(1.2)
        assert recipe_stacks() == original
        page = aei()['recipe_page']
        ctl('key','right','tap')
        following = recipe_stacks()
        assert following != original
        assert len({s['item'] for s in following[:-1] if s}) == 1, following
        ctl('key','left','tap')
        assert recipe_stacks() == original
        ctl('scroll-at',x,y,-1)
        assert recipe_stacks() == following
        ctl('scroll-at',x,y,1)
        assert recipe_stacks() == original and aei()['recipe_page'] == page
        ctl('screenshot',str(folder/'aei-cycle-paused.png'))
        ctl('key','shift','up')
        assert not aei()['cycle_paused']
        time.sleep(1.1)
        assert recipe_stacks() != original
        # The hold control can be rebound to a normal key and back to a modifier alone.
        playing()
        ctl('options','aei')
        click_label('Left Shift')
        ctl('key','space','tap')
        playing()
        ctl('open-inventory')
        query('minecraft:crafting_table')
        lookup()
        hover_ingredient()
        ctl('key','space','down')
        original = recipe_stacks()
        assert aei()['cycle_paused']
        time.sleep(1.2)
        assert recipe_stacks() == original
        ctl('key','right','tap')
        assert recipe_stacks() != original
        ctl('key','space','up')
        assert not aei()['cycle_paused']
        playing()
        ctl('options','aei')
        click_label('Space')
        ctl('key','shift','down')
        ctl('key','shift','up')
        assert 'horizon.aei.pauseCycle:key.keyboard.left.shift' in (home/'horizon-extras.txt').read_text()
        playing()
        ctl('open-inventory')
        query('minecraft:crafting_table')
        lookup()
        ctl('key','shift','down')
        assert aei()['cycle_paused']
        ctl('key','shift','up')
        # Native station output metadata uses the same index as its ingredient.
        ctl('key','esc','tap')
        query('minecraft:diamond_helmet')
        lookup()
        for _ in range(len(aei()['categories'])):
            if aei()['category_id'] == 'minecraft:smithing':
                break
            ctl('key','shift','down')
            ctl('key','pagedown','tap')
            ctl('key','shift','up')
        assert aei()['category_id'] == 'minecraft:smithing'
        hover_ingredient()
        ctl('key','shift','down')
        original = recipe_stacks()
        assert 'minecraft:trim' in original[-1].get('components',''), original
        ctl('key','right','tap')
        following = recipe_stacks()
        assert following[2] != original[2] and following[-1] != original[-1], (original,following)
        ctl('key','left','tap')
        assert recipe_stacks() == original
        ctl('key','shift','up')
        print(json.dumps({'mode': mode, 'status': 'passed', 'artifacts': str(folder)}), flush=True)
    finally:
        try:
            ctl('quit')
        except (AssertionError, subprocess.TimeoutExpired):
            game.terminate()
        try:
            game.wait(timeout=20)
        except subprocess.TimeoutExpired:
            game.kill()
            game.wait()
        log.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bin-dir', type=Path, default=ROOT / 'target/fast')
    parser.add_argument('--resources', type=Path, default=ROOT / '.local/horizon-client/26.3')
    parser.add_argument('--mod', type=Path, default=ROOT / 'target/mods-test/mods/aei_integration.hmod')
    parser.add_argument('--mode', choices=('both', 'server-thread', 'in-frame'), default='both')
    args = parser.parse_args()
    for field in ('bin_dir', 'resources', 'mod'):
        setattr(args, field, getattr(args, field).resolve())
    assert args.mod.is_file(), 'Build the SDK example first (see --help)'
    output = Path(tempfile.mkdtemp(prefix='horizon-aei-e2e-'))
    print('artifacts:', output, flush=True)
    for mode in (('server-thread', 'in-frame') if args.mode == 'both' else (args.mode,)):
        run(args, mode, output)


if __name__ == '__main__':
    main()
