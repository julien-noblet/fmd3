----------------------------------------------------------------------------------------------------
-- Local Constants
----------------------------------------------------------------------------------------------------

local DirectoryPagination = '/backend/ajax/searchengine.php'
local DirectoryParameters = 'contentType=manga&retrieveCategories=true&retrieveAuthors=true&perPage=18&page='

----------------------------------------------------------------------------------------------------
-- Event Functions
----------------------------------------------------------------------------------------------------

-- Get info and chapter list for current manga.
function GetInfo()
	local u = MaybeFillHost(MODULE.RootURL, URL)
	if not HTTP.GET(u) then return net_problem end

	local x = CreateTXQuery(HTTP.Document)
	MANGAINFO.URL       = x.XPathString('//meta[@property="og:URL"]/@content')
	MANGAINFO.Title     = x.XPathString('//meta[@property="og:title"]/@content')
	MANGAINFO.CoverLink = x.XPathString('//meta[@property="og:image"]/@content')
	MANGAINFO.Status    = MangaInfoStatusIfPos(x.XPathString('//*[@class="infom"]/div/div[2]//p[contains(.,"Estado")]'), 'Activo', 'Finalizado')
	MANGAINFO.Authors   = x.XPathString('//*[@class="infom"]/div/div[2]//p[contains(.,"Autor") and not(contains(.,"No Disponible"))]/substring-after(normalize-space(.),": ")')
	MANGAINFO.Artists   = x.XPathString('//*[@class="infom"]/div/div[2]//p[contains(.,"Artist") and not(contains(.,"No Disponible"))]/substring-after(normalize-space(.),": ")')
	MANGAINFO.Genres    = x.XPathString('//*[@class="panel-footer" and contains(.,"Géneros")]/string-join(.//a,", ")')
	MANGAINFO.Summary   = x.XPathString('//meta[@property="og:description"]/@content')

	local function parseChapters()
		x.XPathHREFAll('//table[contains(@class, "table")]//h4[@class="title"]/a', MANGAINFO.ChapterLinks, MANGAINFO.ChapterNames)
	end	
	parseChapters()

	local body = HTTP.Document.ToString()
	local totCntnts = tonumber(body:match('var totCntnts=(%d+);')) or 1
	local perPage = tonumber(body:match("php_pagination%(.-,.-,.-,.-,.-,0x(.-),"), 16) or 10
	local c = math.ceil(totCntnts / perPage)

	if c > 1 then
		for i = 2, c do
			if HTTP.GET(MANGAINFO.URL .. '/p/' .. i) then
				x.ParseHTML(HTTP.Document)
				parseChapters()
			end
		end
	end

	MANGAINFO.ChapterLinks.Reverse(); MANGAINFO.ChapterNames.Reverse()

	return no_error
end

-- Get the page count of the manga list of the current website.
function GetDirectoryPageNumber()
	local u = MODULE.RootURL .. DirectoryPagination

	if not HTTP.POST(u, DirectoryParameters .. '1') then return net_problem end

	PAGENUMBER = math.ceil(tonumber(CreateTXQuery(HTTP.Document).XPathString('json(*).totalContents')) / 18)

	return no_error
end

-- Get LINKS and NAMES from the manga list of the current website.
function GetNameAndLink()
	local c, x = nil
	local u = MODULE.RootURL .. DirectoryPagination

	if not HTTP.POST(u, DirectoryParameters .. (URL + 1)) then return net_problem end

	x = CreateTXQuery(HTTP.Document)
	c = x.XPathCount('json(*).contents()')

	for i = 1, c do
		NAMES.Add(x.XPathString('json(*).contents(' .. i .. ').name'))
		LINKS.Add(x.XPathString('json(*).contents(' .. i .. ')/concat("/manga/",id,"/",slug)'))
	end

	return no_error
end

-- Get the page count for the current chapter.
function GetPageNumber()
	local c, p, x = nil
	local u = MaybeFillHost(MODULE.RootURL, URL:gsub('/c/', '/leer/'))

	if not HTTP.GET(u) then return net_problem end

	local body = HTTP.Document.ToString()
	local base = body:match('<base href="(.-)"')
	if base == nil then base = MODULE.RootURL end
	local s = body:match('var%s+pUrl%s*=%s*(.-);')
	local i; for i in s:gmatch('imgURL":"(.-)"') do
		TASK.PageLinks.Add(base .. i:gsub('\\',''))
	end
	return no_error
end

function BeforeDownloadImage()
	if TASK.CurrentDownloadChapterPtr < TASK.ChapterLinks.Count then
		HTTP.Headers.Values['Referer'] = ' ' .. MaybeFillHost(MODULE.RootURL, TASK.ChapterLinks[TASK.CurrentDownloadChapterPtr])
	end
	return true
end

----------------------------------------------------------------------------------------------------
-- Module Initialization
----------------------------------------------------------------------------------------------------

function Init()
	local m = NewWebsiteModule()
	m.ID                       = '6138f1a985cd47c3b60a65cf6b1fe03d'
	m.Name                     = 'KuManga'
	m.RootURL                  = 'https://www.kumanga.com'
	m.Category                 = 'Spanish'
	m.OnGetInfo                = 'GetInfo'
	m.OnGetNameAndLink         = 'GetNameAndLink'
	m.OnGetPageNumber          = 'GetPageNumber'
	m.OnGetDirectoryPageNumber = 'GetDirectoryPageNumber'
	m.OnBeforeDownloadImage    = 'BeforeDownloadImage'
end