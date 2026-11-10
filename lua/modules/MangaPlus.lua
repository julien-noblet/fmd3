----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = 'f239e87c7a1248d29cdd2ea8a77df36c'
	m.Name                     = 'MangaPlus'
	m.RootURL                  = 'https://mangaplus.shueisha.co.jp'
	m.Category                 = 'English'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.OnDownloadImage          = 'DownloadImage'

	local slang = require 'fmd.env'.SelectedLanguage
	local translations = {
		['en'] = {
			['imageresolution'] = 'Page resolution:',
			['resolution'] = 'Low\nMedium\nHigh',
			['lang'] = 'Preferred Language:',
			['languages'] = 'All\nEnglish\nFrench\nSpanish\nIndonesian\nPortuguese (BR)\nRussian\nThai\nGerman\nVietnamese'
		},
		['es'] = {
			['imageresolution'] = 'Resolución de página:',
			['resolution'] = 'Bajo\nMedio\nAlto',
			['lang'] = 'Idioma preferido:',
			['languages'] = 'Todos\nInglés\nFrancés\nEspañol\nIndonesio\nPortugués (BR)\nRuso\nTailandés\nAlemán\nVietnamita'
		},
		['fr'] = {
			['imageresolution'] = 'Résolution de la page:',
			['resolution'] = 'Basse\nMoyenne\nHaute',
			['lang'] = 'Langue préférée:',
			['languages'] = 'Toutes\nEnglish\nFrançais\nEspañol\nIndonesian\nPortuguês (BR)\nRussian\nThai\nGerman\nVietnamese'
		}
	}
	local lang = translations[slang] or translations.fr or translations.en
	m.AddOptionComboBox('imageresolution', lang.imageresolution, lang.resolution, 2)
	m.AddOptionComboBox('lang', lang.lang or 'Langue préférée:', lang.languages or 'Toutes\nEnglish\nFrançais\nEspañol\nIndonesian\nPortuguês (BR)\nRussian\nThai\nGerman\nVietnamese', 0)
end

----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

local API_URL = 'https://jumpg-webapi.tokyo-cdn.com/api'
local separator = '↣' -- Save Encryption key in the URL and separate it using obscure char (U+21A3)
local protoc = require 'utils.protoc'
local pb = require 'pb'
local proto_file = 'MangaPlus.proto'

----------------------------------------------------------------------------------------------------
-- Helper Functions
----------------------------------------------------------------------------------------------------

-- Seed random number generator once.
math.randomseed(os.time())

local LANG_MAP = {
	[0] = 'English', ['ENGLISH'] = 'English',
	[1] = 'Español', ['SPANISH'] = 'Español',
	[2] = 'Français', ['FRENCH'] = 'Français',
	[3] = 'Indonesian', ['INDONESIAN'] = 'Indonesian',
	[4] = 'Português (BR)', ['PORTUGUESE_BR'] = 'Português (BR)',
	[5] = 'Русский', ['RUSSIAN'] = 'Русский',
	[6] = 'ภาษาไทย', ['THAI'] = 'ภาษาไทย',
	[7] = 'Deutsch', ['GERMAN'] = 'Deutsch',
	[8] = 'Italiano', ['ITALIAN'] = 'Italiano',
	[9] = 'Tiếng Việt', ['VIETNAMESE'] = 'Tiếng Việt',
}

local LANG_TAGS = {
	[0] = 'EN', ['ENGLISH'] = 'EN',
	[1] = 'ES', ['SPANISH'] = 'ES',
	[2] = 'FR', ['FRENCH'] = 'FR',
	[3] = 'ID', ['INDONESIAN'] = 'ID',
	[4] = 'PT-BR', ['PORTUGUESE_BR'] = 'PT-BR',
	[5] = 'RU', ['RUSSIAN'] = 'RU',
	[6] = 'TH', ['THAI'] = 'TH',
	[7] = 'DE', ['GERMAN'] = 'DE',
	[8] = 'IT', ['ITALIAN'] = 'IT',
	[9] = 'VI', ['VIETNAMESE'] = 'VI',
}

local function GetLang(lang)
	local tag = LANG_TAGS[lang] or 'EN'
	return ' [' .. tag .. ']'
end

local function SplitString(s, delimiter)
	local result = {}
	for match in (s .. delimiter):gmatch('(.-)' .. delimiter) do
		table.insert(result, match)
	end
	return result
end

local function GenerateUUID()
    local template = 'xxxxxxxx-xxxx-1xxx-yxxx-xxxxxxxxxxxx'
    return string.gsub(template, '[xy]', function (c)
        local v = (c == 'x') and math.random(0, 15) or math.random(8, 11)
        return string.format('%x', v)
    end)
end

local function ReadFile(file)
	local f = io.open(file, 'rb')
	if not f then
		f = io.open('lua/modules/' .. proto_file, 'rb')
	end
	if not f then
		f = assert(io.open('../../lua/modules/' .. proto_file, 'rb'))
	end
	local content = f:read('*all')
	f:close()
	return content
end

-- Read File and load it to proto
local curr_path = debug.getinfo(1, 'S').source
local curr_script = curr_path:match('[^\\/]*.lua$')
local target_file = curr_path
if curr_script then
	target_file = curr_path:gsub(curr_script, proto_file):gsub('@', '')
else
	target_file = 'lua/modules/' .. proto_file
end
protoc.proto3_optional = true
protoc:load(ReadFile(target_file))

local all_titles_cache = nil

local function GetAllTitleGroups()
	if all_titles_cache then return all_titles_cache end
	local u = API_URL .. '/title_list/allV2'
	HTTP.Headers.Values['Session-Token'] = GenerateUUID()
	if not HTTP.GET(u) then return nil end
	local decoded = pb.decode('Response', HTTP.Document.ToString())
	if decoded and decoded.success and decoded.success.allTitlesViewV2 then
		all_titles_cache = decoded.success.allTitlesViewV2.AllTitlesGroup
		return all_titles_cache
	end
	return nil
end

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get links and names from the manga list of the current website.
function GetNameAndLink()
	local groups = GetAllTitleGroups()
	if not groups then return net_problem end

	local opt_lang_idx = MODULE.GetOption('lang') or 0
	local target_tag = nil
	if opt_lang_idx == 1 then target_tag = 'EN'
	elseif opt_lang_idx == 2 then target_tag = 'FR'
	elseif opt_lang_idx == 3 then target_tag = 'ES'
	elseif opt_lang_idx == 4 then target_tag = 'ID'
	elseif opt_lang_idx == 5 then target_tag = 'PT-BR'
	elseif opt_lang_idx == 6 then target_tag = 'RU'
	elseif opt_lang_idx == 7 then target_tag = 'TH'
	elseif opt_lang_idx == 8 then target_tag = 'DE'
	elseif opt_lang_idx == 9 then target_tag = 'VI'
	end

	for _, group in ipairs(groups) do
		for _, v in ipairs(group.titles or {}) do
			local tag = LANG_TAGS[v.language] or 'EN'
			if not target_tag or tag == target_tag then
				LINKS.Add('titles/' .. v.titleId)
				NAMES.Add(v.name .. GetLang(v.language))
			end
		end
	end

	return no_error
end

-- Get info and chapter list for the current manga.
function GetInfo()
	local requested_id = tonumber(URL:match('(%d+)'))
	if not requested_id then return net_problem end

	local groups = GetAllTitleGroups()
	local target_id = requested_id
	local siblings = {}

	if groups then
		for _, group in ipairs(groups) do
			local in_group = false
			for _, v in ipairs(group.titles or {}) do
				if v.titleId == requested_id then
					in_group = true
					break
				end
			end
			if in_group then
				for _, v in ipairs(group.titles or {}) do
					local lang_name = LANG_MAP[v.language] or 'English'
					local lang_tag = LANG_TAGS[v.language] or 'EN'
					table.insert(siblings, {
						titleId = v.titleId,
						name = v.name,
						language = v.language,
						label = lang_name .. ' [' .. lang_tag .. ']'
					})
				end
				break
			end
		end
	end

	local opt_lang_idx = MODULE.GetOption('lang') or 0
	local target_tag = nil
	if opt_lang_idx == 1 then target_tag = 'EN'
	elseif opt_lang_idx == 2 then target_tag = 'FR'
	elseif opt_lang_idx == 3 then target_tag = 'ES'
	elseif opt_lang_idx == 4 then target_tag = 'ID'
	elseif opt_lang_idx == 5 then target_tag = 'PT-BR'
	elseif opt_lang_idx == 6 then target_tag = 'RU'
	elseif opt_lang_idx == 7 then target_tag = 'TH'
	elseif opt_lang_idx == 8 then target_tag = 'DE'
	elseif opt_lang_idx == 9 then target_tag = 'VI'
	end

	if target_tag then
		for _, s in ipairs(siblings) do
			if LANG_TAGS[s.language] == target_tag then
				target_id = s.titleId
				break
			end
		end
	end

	local u = API_URL .. '/title_detailV3?title_id=' .. target_id
	HTTP.Headers.Values['Session-Token'] = GenerateUUID()
	
	if not HTTP.GET(u) then return net_problem end

	local decoded = pb.decode('Response', HTTP.Document.ToString())
	if not decoded or not decoded.success or not decoded.success.titleDetailView then return net_problem end
	local manga = decoded.success.titleDetailView

	MANGAINFO.Title     = manga.title.name .. GetLang(manga.title.language)
	MANGAINFO.CoverLink = manga.titleImageUrl
	MANGAINFO.Authors   = manga.title.author
	MANGAINFO.Status    = MangaInfoStatusIfPos(manga.titleLabels.releaseSchedule, 'day|ly|other', 'completed|one_shot')
	MANGAINFO.Summary   = manga.overview

	local genres = {}
	for _, genre in ipairs(manga.tags or {}) do
		table.insert(genres, genre.tag)
	end
	MANGAINFO.Genres = table.concat(genres, ', ')

	-- Expose available language editions
	if MANGAINFO.AlternateVersions and #siblings > 0 then
		for _, s in ipairs(siblings) do
			local alt_url = 'https://mangaplus.shueisha.co.jp/titles/' .. s.titleId
			MANGAINFO.AlternateVersions.Add(s.label, s.name, alt_url)
		end
	end

	local function addChapter(chapter)
		for _, v in ipairs(chapter) do
			local chaptername = v.subTitle
			if chaptername == '' then chaptername = v.name end
			MANGAINFO.ChapterNames.Add(chaptername)
			MANGAINFO.ChapterLinks.Add(tostring(v.chapterId))
		end
	end

	local list_groups = manga.chapterListGroup
	if list_groups then
		for _,v in ipairs(list_groups) do
			local first_list = v.firstChapterList
			if first_list then addChapter(first_list) end
			local last_list = v.lastChapterList
			if last_list then addChapter(last_list) end
		end
	end

	return no_error
end

-- Get the page count and/or page links for the current chapter.
function GetPageNumber()
	local crypto = require 'fmd.crypto'
	local imageresolution = {'low', 'high', 'super_high'}
	local opt_res = MODULE.GetOption and MODULE.GetOption('imageresolution')
	local sel_imageresolution = (tonumber(opt_res) or 2) + 1
	if sel_imageresolution > #imageresolution or sel_imageresolution < 1 then
		sel_imageresolution = 2
	end
	local chapter_id = URL:match('(%d+)') or URL
	local u = API_URL .. '/manga_viewer?chapter_id=' .. chapter_id .. '&img_quality=' .. imageresolution[sel_imageresolution] .. '&split=yes'
	HTTP.Reset()
	HTTP.Headers.Values['Session-Token'] = GenerateUUID()

	if not HTTP.GET(u) then return false end

	local doc_str = HTTP.Document.ToString()
	local decoded = pb.decode('Response', doc_str)
	if not decoded or not decoded.success or not decoded.success.mangaViewer or not decoded.success.mangaViewer.pages then
		return false
	end
	local manga = decoded.success.mangaViewer.pages

	for _, v in ipairs(manga) do
		if v.mangaPage then
			local image_url = v.mangaPage.imageUrl
			local encryption_key = v.mangaPage.encryptionKey
			TASK.PageLinks.Add(image_url .. separator .. encryption_key)
		end
	end
	return true
end

-- Download and decrypt image given the image URL.
function DownloadImage()
	local t = SplitString(URL, separator)
	local url = t[1]
	local key = require 'fmd.crypto'.HexToStr(t[2])

	if not HTTP.GET(url) then return false end

	local manga = HTTP.Document.ToString()
	local parsed = {}
	for i = 1, manga:len() do
		parsed[i] = string.char(string.byte(manga, i) ~ string.byte(key, ((i - 1) % string.len(key)) + 1))
	end
	HTTP.Document.WriteString(table.concat(parsed, ''))

	return true
end
