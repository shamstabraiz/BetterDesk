/**
 * BetterDesk Console - RustDesk address book sync helper tests
 */

const sync = require('../services/rustdeskAddressBookSync');

describe('rustdeskAddressBookSync', () => {
    it('merges panel tags and folders into existing address book peers', () => {
        const result = JSON.parse(sync.mergeAddressBookData(JSON.stringify({
            peers: [{ id: '123456789', tags: ['Existing'] }],
            tags: ['Existing']
        }), {
            devices: [
                { id: '123456789', hostname: 'PC-1', tags: ['Internal'], folder_id: 2 },
                { id: '987654321', hostname: 'PC-2', tags: 'External' }
            ],
            folders: [{ id: 2, name: 'Workstations' }],
            assignments: {},
            includeDevices: true
        }));

        expect(result.tags).toEqual(['Existing', 'Workstations', 'Internal', 'External']);
        expect(result.peers).toHaveLength(2);
        expect(result.peers[0].tags).toEqual(['Existing', 'Internal', 'Workstations']);
        expect(result.peers[1]).toMatchObject({
            id: '987654321',
            hostname: 'PC-2',
            tags: ['External']
        });
    });

    it('does not add new peers when includeDevices is false', () => {
        const result = JSON.parse(sync.mergeAddressBookData('{}', {
            devices: [{ id: '123456789', tags: ['Internal'] }],
            includeDevices: false
        }));

        expect(result.peers).toEqual([]);
        expect(result.tags).toEqual([]);
    });

    it('collects a sorted unique tag list for suggestions', () => {
        const tags = sync.collectVisibleTags(
            [
                { id: '123456789', tags: ['Internal', 'Windows'] },
                { id: '987654321', tags: 'External, Windows', folder_id: 4 }
            ],
            [{ id: 4, name: 'Laptops' }],
            {}
        );

        expect(tags).toEqual(['External', 'Internal', 'Laptops', 'Windows']);
    });
});
